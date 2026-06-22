use std::{
    io::{self, Cursor, ErrorKind},
    net::SocketAddr,
    sync::Arc,
};

use bytes::{Buf, BytesMut};
use statik_core::prelude::*;
use statik_proto::{
    common::{abilities, KnownPack},
    v1_20_1::{c2s::C2SPacket as C2SPacketV1_20_1, s2c::play::S2CKeepAlive as S2CKeepAliveV1_20_1},
    v1_21_1::c2s::C2SPacket as C2SPacketV1_21_1,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::TcpStream,
    sync::RwLock,
};

use crate::{
    config::ServerConfig,
    protocol::{intent_to_state, DecodedC2S, ProtocolKind},
};

/// How often the server sends a Play-state `S2CKeepAlive`. The vanilla client
/// disconnects after ~30s of silence, so 15s gives comfortable headroom.
const KEEPALIVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);

/// Checks if a username COULD be a valid minecraft account's username.
///
/// There is a few possible cases where this won't apply, like the handful
/// of single/double character accounts or the accounts with spaces in them,
/// but they are so rare (and not really applicable to this server's use case)
/// thtat it's not worth considering them here.
fn is_valid_username(username: &str) -> bool {
    (3..=16).contains(&username.len())
        && username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Send and receive `Frame` values from a minecraft client.
///
/// When implementing networking protocols, a message on that protocol is
/// often composed of several smaller messages known as frames. The purpose of
/// `Connection` is to read and write frames on the underlying `TcpStream`.
///
/// To read frames, the `Connection` uses an internal buffer, which is filled
/// up until there are enough bytes to create a full frame. Once this happens,
/// the `Connection` creates the frame and returns it to the caller.
///
/// When sending frames, the frame is first encoded into the write buffer.
/// The contents of the write buffer are then written to the socket.
#[derive(Debug)]
pub struct Connection {
    config: Arc<RwLock<ServerConfig>>,

    /// The selected Minecraft protocol version for this connection. Set at
    /// accept time from the server's `[mc] version` config field and carried
    /// for the connection's lifetime; decode/dispatch route through it.
    pub protocol: ProtocolKind,

    // /// All the data accociated with the client after they have connected, including
    // /// their username, UUID, (in the future) items, ect. Defaults to None, as this data
    // /// isn't sent with a status request, only on login.
    // pub player: Option<Player>,

    // The `TcpStream`. It is decorated with a `BufWriter`, which provides write
    // level buffering. The `BufWriter` implementation provided by Tokio is
    // sufficient for our needs.
    pub stream: BufWriter<TcpStream>,

    /// The address that the connection comes from.
    pub address: SocketAddr,

    /// The buffer for reading frames.
    pub buffer: BytesMut,

    /// Scratch buffer used when encoding outbound packets: a packet's body is
    /// encoded here before being framed with its VarInt length prefix and
    /// written to the stream.
    staging: Vec<u8>,

    /// Current state of the handler: should go from 0 (Handshake) to 1 (status)
    /// or to 2 (login, which then goes to 3 (play))
    pub state: State,

    /// Minecraft packet compression threshold. `None` means compression is
    /// disabled (the default). After `S2CSetCompression` is sent during
    /// login, this is set to `Some(threshold)` and all subsequent packets
    /// in both directions are zlib-compressed when their body length meets
    /// the threshold.
    pub compression_threshold: Option<i32>,
}

impl Connection {
    /// Create a new `Connection`, backed by `socket`. Read and write buffers
    /// are initialized.
    pub async fn new(
        config: Arc<RwLock<ServerConfig>>,
        protocol: ProtocolKind,
        socket: TcpStream,
        address: SocketAddr,
    ) -> Self {
        let max_packet_size = config.read().await.mc.max_packet_size;

        Self {
            config,
            protocol,
            // player: None,
            stream: BufWriter::new(socket),
            address,
            buffer: BytesMut::with_capacity(max_packet_size),
            staging: Vec::with_capacity(max_packet_size),
            state: State::Handshake,
            compression_threshold: None,
        }
    }

    /// Read packets from the underlying stream until the peer closes the
    /// connection.
    ///
    /// The function loops, first draining every complete frame already buffered
    /// and then reading more bytes when a partial frame is encountered. Any
    /// data remaining in the read buffer after a packet has been parsed is kept
    /// there for the next iteration.
    ///
    /// While in Play state, a keepalive timer runs concurrently: every
    /// [`KEEPALIVE_INTERVAL`] the server sends a fresh `S2CKeepAlive` with a
    /// new id. The vanilla client disconnects after ~30s without a keepalive,
    /// so this MUST be server-driven — the client never sends one
    /// unprompted. (Keepalive ids are just monotonic counters; we don't track
    /// acks in limbo.)
    ///
    /// # Returns
    ///
    /// Returns `Err` if the `TcpStream` is closed (mapped to
    /// [`io::ErrorKind::UnexpectedEof`]) or a malformed frame is encountered.
    /// Callers may treat a clean EOF as a normal connection end.
    pub async fn handle_connection(&mut self) -> Result<()> {
        // Only run the keepalive timer once we're in Play state (it'd be
        // pointless — and wrong — during handshake/status/login).
        let mut keepalive = tokio::time::interval(KEEPALIVE_INTERVAL);
        // The first tick fires immediately; skip it so we don't send a
        // keepalive the instant we enter the loop.
        keepalive.tick().await;
        let mut next_keepalive_id: i64 = 0;

        loop {
            trace!("handling connection with {}", self.address);

            // Drain every complete frame currently buffered. `try_parse_packet`
            // returns `Ok(None)` when there is not enough data yet for the next
            // frame, in which case we fall through and read more bytes.
            while let Some(packet) = self.try_parse_packet()? {
                self.dispatch_packet(packet).await?;
            }

            // Race the next read against the keepalive timer. The read branch
            // is `Ok(0)`-aware (clean EOF) and resumes the loop on partial
            // data; the keepalive branch fires a `S2CKeepAlive` and loops.
            tokio::select! {
                read = self.stream.read_buf(&mut self.buffer) => {
                    let bytes_read = read?;
                    if bytes_read == 0 {
                        return Err(io::Error::from(ErrorKind::UnexpectedEof).into());
                    }
                    trace!("read {bytes_read} bytes from {}.", self.address);
                }
                _ = keepalive.tick(), if self.state == State::Play => {
                    let id = next_keepalive_id;
                    next_keepalive_id = next_keepalive_id.wrapping_add(1);
                    trace!(
                        "sending keepalive id {id} to {} (Play keepalive timer)",
                        self.address
                    );
                    // KeepAlive wire format is identical in 1.20.1 and 1.21.1
                    // (`id: i64`); use the 1.20.1 struct since it is
                    // protocol-equivalent.
                    self.write_packet(S2CKeepAliveV1_20_1 { id }).await?;
                }
            }
        }
    }

    /// Attempts to parse a single packet frame from the read buffer without
    /// blocking.
    ///
    /// Returns:
    /// - `Ok(Some(packet))` when a full frame was buffered; the frame's bytes
    ///   are consumed from `self.buffer`.
    /// - `Ok(None)` when the buffer does not yet contain a complete frame
    ///   (either the length VarInt or the packet body is still partial). The
    ///   caller should read more bytes and retry.
    /// - `Err` when the buffered data is malformed (bad VarInt, declared length
    ///   out of bounds, decode failure).
    fn try_parse_packet(&mut self) -> Result<Option<DecodedC2S>> {
        // Peek at the leading length VarInt without committing to consuming it.
        let mut cursor = Cursor::new(&self.buffer[..]);
        let length = match VarInt::decode(&mut cursor) {
            Ok(v) => v.0,
            Err(e) => {
                // A partial length VarInt surfaces as an UnexpectedEof from
                // `read_u8`; that just means we need more bytes, not an error.
                if let Some(io_err) = e.downcast_ref::<io::Error>() {
                    if io_err.kind() == ErrorKind::UnexpectedEof {
                        return Ok(None);
                    }
                }
                return Err(e);
            }
        };

        ensure!(
            length >= 0,
            "packet length must be non-negative, got {length}"
        );
        ensure!(
            length as usize <= MAX_PACKET_SIZE as usize,
            "declared packet length {length} exceeds MAX_PACKET_SIZE ({})",
            MAX_PACKET_SIZE
        );

        let header_len = cursor.position() as usize;
        let length = length as usize;

        // Not enough body buffered yet.
        if self.buffer.len() < header_len + length {
            return Ok(None);
        }

        // Consume the length VarInt, then decode the packet body (which itself
        // begins with the packet-id VarInt handled by `C2SPacket::decode`).
        self.buffer.advance(header_len);

        // If compression is enabled, the packet body starts with a VarInt
        // `Data Length` of the uncompressed payload. If 0 the rest is
        // uncompressed; otherwise zlib-decompress that many bytes before
        // decoding. Buffer is `Vec<u8>` so we can hand it to `Cursor` for
        // `decode_in_state`.
        let body_slice = &self.buffer[..length];
        let mut decompressed: Vec<u8>;
        let body_bytes: &[u8] = if let Some(threshold) = self.compression_threshold {
            let mut peek = Cursor::new(body_slice);
            let data_len = match VarInt::decode(&mut peek) {
                Ok(v) => v.0,
                Err(e) => {
                    if let Some(io_err) = e.downcast_ref::<io::Error>() {
                        if io_err.kind() == ErrorKind::UnexpectedEof {
                            return Ok(None);
                        }
                    }
                    return Err(e);
                }
            };
            let data_len = data_len as usize;
            let compressed_start = peek.position() as usize;
            if data_len == 0 {
                // Uncompressed (body below threshold): skip the 0 VarInt and
                // use the rest as-is.
                &body_slice[compressed_start..]
            } else {
                ensure!(
                    data_len >= threshold as usize,
                    "compressed packet Data Length {data_len} is below threshold {threshold}"
                );
                ensure!(
                    data_len <= MAX_PACKET_SIZE as usize,
                    "uncompressed Data Length {data_len} exceeds MAX_PACKET_SIZE"
                );
                use std::io::Read as _;

                use flate2::read::ZlibDecoder;
                let mut decoder = ZlibDecoder::new(&body_slice[compressed_start..]);
                decompressed = vec![0u8; data_len];
                decoder
                    .read_exact(&mut decompressed)
                    .map_err(|e| anyhow!("zlib decode failed: {e}"))?;
                // Hand `decompressed` to the decoder below.
                &decompressed
            }
        } else {
            body_slice
        };

        let decode_result = {
            let mut body = Cursor::new(body_bytes);
            DecodedC2S::decode(self.protocol, self.state, &mut body)
        };

        // The frame is fully delimited by `length`, so consume it from the
        // buffer regardless of whether decoding succeeded.
        self.buffer.advance(length);

        match decode_result {
            Ok(packet) => {
                debug!("(↓) packet recieved: {:?}", &packet);
                Ok(Some(packet))
            }
            // In Play / Configuration state we only model a subset of packets;
            // the vanilla client sends many we don't. An unrecognized/unparseable
            // frame must be skipped rather than treated as fatal, or the client
            // gets disconnected the instant it enters limbo. We've already
            // advanced past the frame, so just report "no packet this round"
            // and let the caller read on.
            Err(e) if self.state == State::Play || self.state == State::Configuration => {
                debug!("(↓) ignoring undecodable {:?} packet: {e}", self.state);
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Routes a decoded packet to the handler for the connection's current
    /// state.
    async fn dispatch_packet(&mut self, packet: DecodedC2S) -> Result<()> {
        match self.state {
            State::Handshake => self.handle_handshake(packet).await,
            State::Status => self.handle_status(packet).await,
            State::Login => self.handle_login(packet).await,
            State::Configuration => self.handle_configuration(packet).await,
            State::Play => self.handle_play(packet).await,
        }
    }

    pub async fn handle_handshake(&mut self, packet: DecodedC2S) -> Result<()> {
        // The handshake packet shape is the same in 1.20.1 and 1.21.1 (same
        // field order / types / wire id); we just unwrap the versioned
        // variant and validate the protocol.
        let (protocol_version, next_state) = match &packet {
            DecodedC2S::V1_20_1(p) => match p {
                C2SPacketV1_20_1::Handshake(h) => (h.protocol_version, h.next_state),
                _ => {
                    return Err(anyhow!(
                        "Recieved a non handshake packet in the handshake stage!"
                    ))
                }
            },
            DecodedC2S::V1_21_1(p) => match p {
                C2SPacketV1_21_1::Handshake(h) => (h.protocol_version, h.next_state),
                _ => {
                    return Err(anyhow!(
                        "Recieved a non handshake packet in the handshake stage!"
                    ))
                }
            },
        };
        let server_protocol = self.protocol.protocol_version();
        if protocol_version.0 as usize != server_protocol {
            return Err(anyhow!(
                "Protocol versions do not match! Client had protocol version: {}, while the \
                 server's selected protocol version is {} ({}).",
                protocol_version.0,
                server_protocol,
                self.protocol.minecraft_version(),
            ));
        };

        // Map the handshake intention onto a connection state. Transfer is
        // rejected here; Configuration is NEVER entered via handshake (only
        // post-LoginSuccess via Login Acknowledged).
        self.state = intent_to_state(next_state)?;
        Ok(())
    }

    pub async fn handle_status(&mut self, packet: DecodedC2S) -> Result<()> {
        let config = self.config.read().await;
        let version_name = self.protocol.minecraft_version();
        let version_protocol = self.protocol.protocol_version();
        let max_players = config.mc.max_players;
        let description = Chat::new(config.mc.motd.clone());
        let icon = config.mc.icon.clone();
        drop(config);

        match packet {
            DecodedC2S::V1_20_1(p) => {
                use statik_proto::v1_20_1::s2c::status::{
                    response::{
                        Players as PlayersV1_20_1, StatusResponse as StatusResponseV1_20_1,
                    },
                    S2CPong, S2CStatusResponse,
                };
                let players = PlayersV1_20_1::new(max_players, 0, vec![]);
                match p {
                    C2SPacketV1_20_1::StatusRequest(_) => {
                        let status_response = S2CStatusResponse {
                            json_response: StatusResponseV1_20_1::new(
                                version_name,
                                version_protocol,
                                players,
                                description,
                                icon,
                                false,
                            ),
                        };
                        self.write_packet(status_response).await?;
                        Ok(())
                    }
                    C2SPacketV1_20_1::Ping(ping) => {
                        let pong = S2CPong {
                            payload: ping.payload,
                        };
                        self.write_packet(pong).await?;
                        Ok(())
                    }
                    _ => Err(anyhow!("Recieved a non status packet in the status stage!")),
                }
            }
            DecodedC2S::V1_21_1(p) => {
                use statik_proto::v1_21_1::s2c::status::{
                    response::{
                        Players as PlayersV1_21_1, StatusResponse as StatusResponseV1_21_1,
                    },
                    S2CPong, S2CStatusResponse,
                };
                let players = PlayersV1_21_1::new(max_players, 0, vec![]);
                match p {
                    C2SPacketV1_21_1::StatusRequest(_) => {
                        let status_response = S2CStatusResponse {
                            json_response: StatusResponseV1_21_1::new(
                                version_name,
                                version_protocol,
                                players,
                                description,
                                icon,
                                false,
                            ),
                        };
                        self.write_packet(status_response).await?;
                        Ok(())
                    }
                    C2SPacketV1_21_1::Ping(ping) => {
                        let pong = S2CPong {
                            payload: ping.payload,
                        };
                        self.write_packet(pong).await?;
                        Ok(())
                    }
                    _ => Err(anyhow!("Recieved a non status packet in the status stage!")),
                }
            }
        }
    }

    pub async fn handle_login(&mut self, packet: DecodedC2S) -> Result<()> {
        use uuid::Uuid;

        // Snapshot the limbo + compression config once at the top — every
        // code path below needs some subset of it.
        let limbo_pos;
        let limbo_gamemode;
        let limbo_view;
        let limbo_sim;
        let limbo_dimension;
        let compression_enabled;
        let compression_threshold;
        let max_players;
        {
            let cfg = self.config.read().await;
            limbo_pos = cfg.limbo.position;
            limbo_gamemode = cfg.limbo.gamemode;
            limbo_view = cfg.limbo.view_distance;
            limbo_sim = cfg.limbo.simulation_distance;
            limbo_dimension = cfg.limbo.dimension.clone();
            compression_enabled = cfg.compression.enabled;
            compression_threshold = cfg.compression.threshold;
            max_players = cfg.mc.max_players;
        }

        match packet {
            DecodedC2S::V1_20_1(p) => {
                use statik_proto::v1_20_1::s2c::{
                    login::{S2CDisconnect, S2CLoginSuccess, S2CSetCompression},
                    play::{
                        registry_bytes, void_chunk_bytes, S2CGameEvent, S2CLevelChunkWithLight,
                        S2CLogin, S2CPlayerAbilities, S2CPlayerPosition, S2CSetChunkCacheCenter,
                        S2CSetChunkCacheRadius, S2CSetDefaultSpawnPosition,
                    },
                };
                match p {
                    C2SPacketV1_20_1::LoginStart(login_start) => {
                        if !is_valid_username(&login_start.username) {
                            warn!(
                                "Rejected login from {}: invalid username \"{}\".",
                                self.address, login_start.username
                            );
                            self.write_packet(S2CDisconnect {
                                reason: Chat::new("Invalid username."),
                            })
                            .await?;
                            return Ok(());
                        }

                        info!(
                            "Player \"{}\" (from {}) entering limbo at ({}, {}, {})",
                            login_start.username,
                            self.address,
                            limbo_pos[0],
                            limbo_pos[1],
                            limbo_pos[2]
                        );

                        // 1. Optional compression (must precede LoginSuccess).
                        if compression_enabled {
                            self.write_packet(S2CSetCompression {
                                threshold: VarInt(compression_threshold),
                            })
                            .await?;
                            self.compression_threshold = Some(compression_threshold);
                        }

                        // 2. LoginSuccess.
                        self.write_packet(S2CLoginSuccess {
                            uuid: Uuid::nil(),
                            username: login_start.username.clone(),
                            properties: vec![],
                        })
                        .await?;

                        // 3. Transition to Play and emit the Play burst.
                        self.state = State::Play;
                        let reg_bytes = registry_bytes().to_vec();
                        self.write_packet(S2CLogin {
                            player_id: 0,
                            hardcore: false,
                            game_type: VarInt(limbo_gamemode),
                            previous_game_type: VarInt(limbo_gamemode),
                            levels: vec![limbo_dimension.clone()],
                            registry_holder: RawBytes(reg_bytes.clone().into()),
                            // `dimension_type` MUST be a key registered in the embedded
                            // dimension_type registry (overworld / overworld_caves /
                            // the_end / the_nether) — `minecraft:the_void` is NOT in the
                            // vanilla codec, so we always use overworld here. `dimension`
                            // (the level id) is free-form and can stay as configured.
                            dimension_type: "minecraft:overworld".to_string(),
                            dimension: limbo_dimension.clone(),
                            seed: 0,
                            max_players: VarInt(max_players),
                            chunk_radius: VarInt(limbo_view),
                            simulation_distance: VarInt(limbo_sim),
                            reduced_debug_info: true,
                            show_death_screen: false,
                            is_debug: false,
                            is_flat: false,
                            last_death_location: None,
                            portal_cooldown: VarInt(0),
                        })
                        .await?;
                        self.write_packet(S2CSetChunkCacheCenter {
                            x: VarInt(0),
                            z: VarInt(0),
                        })
                        .await?;
                        self.write_packet(S2CSetChunkCacheRadius {
                            radius: VarInt(limbo_view),
                        })
                        .await?;
                        self.write_packet(S2CLevelChunkWithLight {
                            payload: RawBytes(void_chunk_bytes().to_vec().into()),
                        })
                        .await?;
                        self.write_packet(S2CSetDefaultSpawnPosition {
                            location: BlockPos::new(
                                limbo_pos[0] as i32,
                                limbo_pos[1] as i32,
                                limbo_pos[2] as i32,
                            ),
                            angle: 0.0,
                        })
                        .await?;
                        self.write_packet(S2CPlayerPosition {
                            x: limbo_pos[0],
                            y: limbo_pos[1],
                            z: limbo_pos[2],
                            y_rot: 0.0,
                            x_rot: 0.0,
                            relative_arguments: 0,
                            id: VarInt(0),
                        })
                        .await?;
                        self.write_packet(S2CPlayerAbilities {
                            flags: abilities::INVULNERABLE | abilities::FLYING | abilities::CAN_FLY,
                            flying_speed: 0.05,
                            walking_speed: 0.1,
                        })
                        .await?;
                        // event = 12 = LEVEL_CHUNKS_LOAD_START. Sending this
                        // last (after the chunk is loaded) lets the client
                        // exit the "Loading Terrain" screen immediately.
                        //
                        // NB: this ordinal is **version-specific**. In 1.20.1
                        // `ClientboundGameEventPacket.Type.LEVEL_CHUNKS_LOAD_START` is
                        // 12. 1.20.2 inserted `LIMITED_CRAFTING` at 12, bumping this to
                        // 13 — using 13 on a 1.20.1 client is an unknown event id and
                        // the client silently stays stuck on "Loading terrain".
                        self.write_packet(S2CGameEvent {
                            event: 12,
                            param: 0.0,
                        })
                        .await?;
                        Ok(())
                    }
                    C2SPacketV1_20_1::EncryptionResponse(_) => bail!(
                        "Received EncryptionResponse from {} but statik never sends an \
                         EncryptionRequest; encryption is not supported.",
                        self.address
                    ),
                    C2SPacketV1_20_1::LoginPluginResponse(_) => bail!(
                        "Received LoginPluginResponse from {} but statik never sends a \
                         LoginPluginRequest; plugin login is not supported.",
                        self.address
                    ),
                    other => bail!("Received a non-login packet in the login stage: {other:?}"),
                }
            }
            DecodedC2S::V1_21_1(p) => {
                use statik_proto::v1_21_1::s2c::{
                    login::{S2CDisconnect, S2CLoginSuccess, S2CSetCompression},
                    play::void_chunk_bytes as void_chunk_bytes_v1_21_1,
                };
                match p {
                    C2SPacketV1_21_1::Hello(hello) => {
                        if !is_valid_username(&hello.name) {
                            warn!(
                                "Rejected login from {}: invalid username \"{}\".",
                                self.address, hello.name
                            );
                            self.write_packet(S2CDisconnect {
                                reason: Chat::new("Invalid username."),
                            })
                            .await?;
                            return Ok(());
                        }

                        info!(
                            "Player \"{}\" (from {}) entering limbo (1.21.1) at ({}, {}, {})",
                            hello.name, self.address, limbo_pos[0], limbo_pos[1], limbo_pos[2]
                        );

                        // 1. Optional compression (must precede LoginSuccess).
                        if compression_enabled {
                            self.write_packet(S2CSetCompression {
                                threshold: VarInt(compression_threshold),
                            })
                            .await?;
                            self.compression_threshold = Some(compression_threshold);
                        }

                        // 2. LoginSuccess — note we STAY in `State::Login`
                        // here; the client must send `Login Acknowledged`
                        // (handled below) to enter Configuration.
                        //
                        // `strict_error_handling = false` matches the
                        // vanilla server (lenient mode); see Mojang
                        // `ClientboundGameProfilePacket`.
                        self.write_packet(S2CLoginSuccess {
                            uuid: Uuid::nil(),
                            username: hello.name.clone(),
                            properties: vec![],
                            strict_error_handling: false,
                        })
                        .await?;

                        // Touch void_chunk_bytes via the v1_21_1 re-export to
                        // make the dep explicit (used in handle_configuration's
                        // Play burst for the LevelChunkWithLight packet).
                        let _ = void_chunk_bytes_v1_21_1;
                        Ok(())
                    }
                    C2SPacketV1_21_1::LoginAcknowledged(_) => {
                        // Transition into Configuration. Send the brand +
                        // feature flags + registry data + known packs up
                        // front, then process the client's response.
                        self.state = State::Configuration;
                        self.send_configuration_burst().await?;
                        Ok(())
                    }
                    C2SPacketV1_21_1::Key(_) => bail!(
                        "Received Key from {} but statik never sends an EncryptionRequest; \
                         encryption is not supported.",
                        self.address
                    ),
                    C2SPacketV1_21_1::CustomQueryAnswer(_) => bail!(
                        "Received Custom Query Answer from {} but statik never sends a Login \
                         Plugin Request; plugin login is not supported.",
                        self.address
                    ),
                    C2SPacketV1_21_1::CookieResponse(_) => bail!(
                        "Received Login Cookie Response from {} but statik never sent a Login \
                         Cookie Request; cookie storage is not supported.",
                        self.address
                    ),
                    other => {
                        bail!("Received a non-login packet in the 1.21.1 login stage: {other:?}")
                    }
                }
            }
        }
    }

    /// Handle a packet received while in Configuration state.
    ///
    /// Configuration state is only entered on the 1.21.1 flow after
    /// `LoginSuccess` + `Login Acknowledged`. We first send the Configuration
    /// burst (brand, feature flags, registry data, known packs), then
    /// process C2S Configuration packets until we see `FinishConfiguration`
    /// — at which point we echo it back, transition to Play, and emit the
    /// 1.21.1 Play burst.
    pub async fn handle_configuration(&mut self, packet: DecodedC2S) -> Result<()> {
        let DecodedC2S::V1_21_1(packet) = packet else {
            // Only 1.21.1 uses Configuration. A 1.20.1 client cannot send
            // any packet in this state — treat as fatal protocol mismatch.
            return Err(anyhow!(
                "Received a configuration-state packet from a 1.20.1 client; this is a protocol \
                 violation."
            ));
        };
        use statik_proto::v1_21_1::s2c::configuration::S2CFinishConfiguration;
        match packet {
            C2SPacketV1_21_1::ClientInformation(info) => {
                debug!(
                    "1.21.1 client information: locale={:?} view={} main_hand={:?}",
                    info.locale, info.view_distance, info.main_hand
                );
                Ok(())
            }
            C2SPacketV1_21_1::SelectKnownPacks(p) => {
                debug!("1.21.1 select known packs: {} pack(s)", p.known_packs.len());
                Ok(())
            }
            C2SPacketV1_21_1::FinishConfigurationAck(_) => {
                // Send the echo, transition to Play, emit the 1.21.1 burst.
                self.write_packet(S2CFinishConfiguration {}).await?;
                self.state = State::Play;
                self.send_play_burst_v1_21_1().await?;
                Ok(())
            }
            C2SPacketV1_21_1::ConfigurationCustomPayload(p) => {
                trace!(
                    "ignored configuration custom payload (channel={:?})",
                    p.channel
                );
                Ok(())
            }
            C2SPacketV1_21_1::ConfigurationKeepAlive(k) => {
                trace!(
                    "ignored configuration keepalive id {} (1.21.1 limbo has no Configuration \
                     keepalive timer)",
                    k.id
                );
                Ok(())
            }
            C2SPacketV1_21_1::PongConfiguration(p) => {
                trace!("ignored configuration pong id {}", p.id);
                Ok(())
            }
            C2SPacketV1_21_1::ResourcePackResponse(r) => {
                trace!("ignored resource pack response {:?}", r.result);
                Ok(())
            }
            other => {
                trace!("ignored 1.21.1 configuration packet: {other:?}");
                Ok(())
            }
        }
    }

    /// Send the 1.21.1 Configuration handshake burst (after `Login
    /// Acknowledged`, before processing C2S Configuration packets).
    ///
    /// Sequence: `Custom Payload (brand)` → `Feature Flags` → `Registry
    /// Data` × N → `Known Packs`. Registry Data uses a stage-2 placeholder
    /// (empty NBT compound) for each known registry; stage 3 replaces them
    /// with real 1.21.1 server captures.
    async fn send_configuration_burst(&mut self) -> Result<()> {
        use statik_proto::v1_21_1::s2c::configuration::{
            S2CCustomPayload, S2CFeatureFlags, S2CKnownPacks, S2CRegistryData,
        };

        // 1. Server brand via the "minecraft:brand" plugin channel.
        // Wire body: VarInt(7) || b"statik" — matches the format vanilla
        // servers send. The brand string length is encoded as a VarInt.
        let brand_bytes = {
            let mut v = Vec::new();
            statik_core::varint::VarInt(6).encode(&mut v)?;
            v.extend_from_slice(b"statik");
            v
        };
        self.write_packet(S2CCustomPayload {
            channel: "minecraft:brand".to_string(),
            data: RawBytes(brand_bytes.into()),
        })
        .await?;

        // 2. Feature flags — the vanilla client only requires
        // "minecraft:vanilla" to be present.
        self.write_packet(S2CFeatureFlags {
            features: vec!["minecraft:vanilla".to_string()],
        })
        .await?;

        // 3. Registry data — one S2CRegistryData packet per registry. The
        // 1.21.1 vanilla client requires all of these registries to be
        // streamed before it leaves the Configuration state; the blobs
        // come from PrismarineJS minecraft-data (captured from a real
        // 1.21.1 server) and are embedded via
        // `statik_proto::v1_21_1::registries::all()`.
        for (registry_id, blob_fn) in statik_proto::v1_21_1::registries::all() {
            self.write_packet(S2CRegistryData {
                registry_id: (*registry_id).to_string(),
                data: RawBytes(blob_fn().to_vec().into()),
            })
            .await?;
        }

        // 4. Known packs — the vanilla core pack. (Stage 2: ship a single
        // known pack so the client can complete the handshake; the
        // version string is best-effort and may need verification in
        // stage 3 against a real 1.21.1 server.)
        self.write_packet(S2CKnownPacks {
            packs: vec![KnownPack {
                namespace: "minecraft".to_string(),
                id: "core".to_string(),
                version: "1.21.1".to_string(),
            }],
        })
        .await?;
        Ok(())
    }

    /// Send the 1.21.1 Play burst (after the Configuration handshake has
    /// completed). Order matches 1.20.1 but with the new packet ids and a
    /// substantially restructured `S2CLogin` packet (the registry is no
    /// longer inline — it was streamed during Configuration).
    async fn send_play_burst_v1_21_1(&mut self) -> Result<()> {
        use statik_proto::v1_21_1::s2c::play::{
            void_chunk_bytes, S2CGameEvent, S2CLevelChunkWithLight, S2CLogin, S2CPlayerAbilities,
            S2CPlayerPosition, S2CSetChunkCacheCenter, S2CSetChunkCacheRadius,
            S2CSetDefaultSpawnPosition, SpawnInfo,
        };

        let limbo_pos;
        let limbo_view;
        let limbo_sim;
        let limbo_dimension;
        let limbo_gamemode;
        let max_players;
        {
            let cfg = self.config.read().await;
            limbo_pos = cfg.limbo.position;
            limbo_view = cfg.limbo.view_distance;
            limbo_sim = cfg.limbo.simulation_distance;
            limbo_dimension = cfg.limbo.dimension.clone();
            limbo_gamemode = cfg.limbo.gamemode;
            max_players = cfg.mc.max_players;
        }

        // 1. S2CLogin (0x2B). Fully modeled — no more RawBytes placeholder.
        // Field layout matches PrismarineJS `play.toClient.packet_login`
        // (verified against `tmp/minecraft-data/data/pc/1.21.1/protocol.json`).
        //
        // The inner `SpawnInfo` carries dimension / gamemode / flatness;
        // we set `is_flat = true` so the void chunk's empty palette renders
        // as flat ground rather than a missing-chunk void.
        //
        // `gamemode` is signed i8 (PrismarineJS mapper { 0=survival,
        // 1=creative, 2=adventure, 3=spectator }), `previous_gamemode = 255`
        // is Mojang's "no previous gamemode" sentinel.
        self.write_packet(S2CLogin {
            entity_id: 0,
            is_hardcore: false,
            world_names: vec![limbo_dimension.clone()],
            max_players: VarInt(max_players),
            view_distance: VarInt(limbo_view),
            simulation_distance: VarInt(limbo_sim),
            reduced_debug_info: false,
            enable_respawn_screen: false,
            do_limited_crafting: false,
            world_state: SpawnInfo {
                dimension: VarInt(0),
                name: limbo_dimension.clone(),
                hashed_seed: 0,
                gamemode: limbo_gamemode as i8,
                previous_gamemode: 0xff,
                is_debug: false,
                is_flat: true,
                death: None,
                portal_cooldown: VarInt(0),
            },
            enforces_secure_chat: false,
        })
        .await?;

        // 2. Chunk cache setup (0x54 / 0x55 in 1.21.1).
        self.write_packet(S2CSetChunkCacheCenter {
            x: VarInt(0),
            z: VarInt(0),
        })
        .await?;
        self.write_packet(S2CSetChunkCacheRadius {
            radius: VarInt(limbo_view),
        })
        .await?;

        // 3. Level Chunk With Light (0x27) — same wire format as 1.20.1.
        self.write_packet(S2CLevelChunkWithLight {
            payload: RawBytes(void_chunk_bytes().to_vec().into()),
        })
        .await?;

        // 4. Set Default Spawn Position (0x56).
        self.write_packet(S2CSetDefaultSpawnPosition {
            location: BlockPos::new(
                limbo_pos[0] as i32,
                limbo_pos[1] as i32,
                limbo_pos[2] as i32,
            ),
            angle: 0.0,
        })
        .await?;

        // 5. Player Position (0x40).
        self.write_packet(S2CPlayerPosition {
            x: limbo_pos[0],
            y: limbo_pos[1],
            z: limbo_pos[2],
            y_rot: 0.0,
            x_rot: 0.0,
            relative_arguments: 0,
            id: VarInt(0),
        })
        .await?;

        // 6. Player Abilities (0x38) — same single-byte bitfield.
        self.write_packet(S2CPlayerAbilities {
            flags: abilities::INVULNERABLE | abilities::FLYING | abilities::CAN_FLY,
            flying_speed: 0.05,
            walking_speed: 0.1,
        })
        .await?;

        // 7. Game Event (0x22) — START_WAITING_FOR_LEVELS. event value 7
        // is the same in 1.20.1 and 1.21.1 (Mojang
        // `ClientboundGameEventPacket.Type.START_WAITING_FOR_LEVELS` — index 7).
        self.write_packet(S2CGameEvent {
            event: 7,
            param: 0.0,
        })
        .await?;
        Ok(())
    }

    /// Handle a packet received while in Play state.
    ///
    /// In limbo we accept teleport acks silently, echo back the client's
    /// keepalive acks (the server also drives its own keepalive timer in
    /// [`handle_connection`]; the client's `C2SKeepAlive` is just the reply),
    /// and ignore the client's position updates — flying mode means the player
    /// won't fall, and the server doesn't actually care where they think they
    /// are.
    pub async fn handle_play(&mut self, packet: DecodedC2S) -> Result<()> {
        match packet {
            DecodedC2S::V1_20_1(p) => match p {
                C2SPacketV1_20_1::AcceptTeleportation(t) => {
                    debug!("client accepted teleport id {:?}", t.id);
                    Ok(())
                }
                C2SPacketV1_20_1::KeepAlive(k) => {
                    trace!("client acked keepalive id {} from {}", k.id, self.address);
                    Ok(())
                }
                C2SPacketV1_20_1::PlayerPos(_)
                | C2SPacketV1_20_1::PlayerPosRot(_)
                | C2SPacketV1_20_1::PlayerRot(_) => {
                    trace!("ignored position update from {}", self.address);
                    Ok(())
                }
                other => {
                    trace!("ignored play packet: {other:?}");
                    Ok(())
                }
            },
            DecodedC2S::V1_21_1(p) => match p {
                C2SPacketV1_21_1::AcceptTeleportation(t) => {
                    debug!("client accepted teleport id {:?}", t.id);
                    Ok(())
                }
                C2SPacketV1_21_1::KeepAlive(k) => {
                    trace!("client acked keepalive id {} from {}", k.id, self.address);
                    Ok(())
                }
                C2SPacketV1_21_1::PlayerPos(_)
                | C2SPacketV1_21_1::PlayerPosRot(_)
                | C2SPacketV1_21_1::PlayerRot(_)
                | C2SPacketV1_21_1::PlayerFlying(_) => {
                    trace!("ignored position update from {}", self.address);
                    Ok(())
                }
                other => {
                    trace!("ignored play packet: {other:?}");
                    Ok(())
                }
            },
        }
    }

    /// Encodes `packet` and writes it to the stream, framed with a leading
    /// VarInt length prefix: `[VarInt(length), packet-id VarInt, fields...]`.
    ///
    /// The packet body is encoded into `staging` first so its length is known,
    /// then the length prefix (at most 5 bytes, encoded on the stack) and the
    /// body are written to the buffered stream and flushed.
    ///
    /// When `compression_threshold` is set (compression negotiated), packets
    /// meeting the threshold are zlib-compressed and prefixed with a VarInt
    /// `Data Length` (= uncompressed size). Sub-threshold packets are sent
    /// with `Data Length = 0` and the raw body.
    pub async fn write_packet(&mut self, packet: impl Packet) -> Result<()> {
        self.staging.clear();
        packet.encode(&mut self.staging)?;

        // Assemble the (possibly compressed) framed body in a new buffer.
        let mut framed = Vec::with_capacity(self.staging.len() + 5);
        if let Some(threshold) = self.compression_threshold {
            if self.staging.len() >= threshold as usize {
                use std::io::Write as _;

                use flate2::{write::ZlibEncoder, Compression};
                let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
                encoder
                    .write_all(&self.staging)
                    .map_err(|e| anyhow!("zlib encode failed: {e}"))?;
                let compressed = encoder
                    .finish()
                    .map_err(|e| anyhow!("zlib finish failed: {e}"))?;
                VarInt(self.staging.len() as i32).encode(&mut framed)?;
                framed.extend_from_slice(&compressed);
            } else {
                // Below threshold: still send the Data Length VarInt (0).
                VarInt(0).encode(&mut framed)?;
                framed.extend_from_slice(&self.staging);
            }
        } else {
            framed.extend_from_slice(&self.staging);
        }

        // Length prefix (at most 5 bytes) covers the entire framed body
        // (which includes the optional Data Length VarInt).
        let mut len_buf = [0u8; 5];
        let mut len_cursor = std::io::Cursor::new(&mut len_buf[..]);
        VarInt(framed.len() as i32).encode(&mut len_cursor)?;
        let len_bytes = len_cursor.position() as usize;

        // One-line summary of the outbound packet. The `id()` accessor is
        // generated by the `Packet` derive.
        debug!(
            "(↑) sent packet 0x{:02x} ({} framed bytes)",
            packet.id().0,
            framed.len()
        );

        self.stream.write_all(&len_buf[..len_bytes]).await?;
        self.stream.write_all(&framed).await?;
        self.stream.flush().await?;

        Ok(())
    }
}
