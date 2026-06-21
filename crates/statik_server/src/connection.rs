use std::{
    io::{self, Cursor, ErrorKind},
    net::SocketAddr,
    sync::Arc,
};

use bytes::{Buf, BytesMut};
use statik_core::prelude::*;
use statik_proto::{
    c2s::C2SPacket,
    s2c::{
        play::S2CKeepAlive,
        status::response::{Players, StatusResponse},
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::TcpStream,
    sync::RwLock,
};

use crate::config::ServerConfig;

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
        socket: TcpStream,
        address: SocketAddr,
    ) -> Self {
        let max_packet_size = config.read().await.mc.max_packet_size;

        Self {
            config,
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
                    self.write_packet(S2CKeepAlive { id }).await?;
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
    fn try_parse_packet(&mut self) -> Result<Option<C2SPacket>> {
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
            C2SPacket::decode_in_state(self.state, &mut body)
        };

        // The frame is fully delimited by `length`, so consume it from the
        // buffer regardless of whether decoding succeeded.
        self.buffer.advance(length);

        match decode_result {
            Ok(packet) => {
                debug!("(↓) packet recieved: {:?}", &packet);
                Ok(Some(packet))
            }
            // In Play state we only model a handful of packets; the vanilla
            // client sends many we don't (Client Information 0x08, Plugin
            // Message, etc.). An unrecognized/unparseable frame must be
            // skipped rather than treated as fatal, or the client gets
            // disconnected the instant it enters limbo. We've already advanced
            // past the frame, so just report "no packet this round" and let
            // the caller read on.
            Err(e) if self.state == State::Play => {
                debug!("(↓) ignoring undecodable play packet: {e}");
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Routes a decoded packet to the handler for the connection's current
    /// state.
    async fn dispatch_packet(&mut self, packet: C2SPacket) -> Result<()> {
        match self.state {
            State::Handshake => self.handle_handshake(packet).await,
            State::Status => self.handle_status(packet).await,
            State::Login => self.handle_login(packet).await,
            State::Play => self.handle_play(packet).await,
        }
    }

    pub async fn handle_handshake(&mut self, packet: C2SPacket) -> Result<()> {
        match packet {
            C2SPacket::Handshake(handshake) => {
                if handshake.protocol_version.0 as usize != PROTOCOL_VERSION {
                    return Err(anyhow!(
                        "Protocol versions do not match! Client had protocol version: {}, while \
                         the server's protocol version is {}.",
                        handshake.protocol_version.0,
                        PROTOCOL_VERSION
                    ));
                };

                let next_state = handshake.next_state;

                self.state = next_state;

                Ok(())
            }
            _ => Err(anyhow!(
                "Recieved a non handshake packet in the handshake stage!"
            )),
        }
    }

    pub async fn handle_status(&mut self, packet: C2SPacket) -> Result<()> {
        use statik_proto::s2c::status::{S2CPong, S2CStatusResponse};
        match packet {
            C2SPacket::StatusRequest(_status_request) => {
                let config = self.config.read().await;

                let status_response = S2CStatusResponse {
                    json_response: StatusResponse::new(
                        Players::new(config.mc.max_players, 0, vec![]),
                        Chat::new(config.mc.motd.clone()),
                        config.mc.icon.clone(),
                        false,
                    ),
                };

                drop(config);

                self.write_packet(status_response).await?;

                Ok(())
            }
            C2SPacket::Ping(ping) => {
                let pong = S2CPong {
                    payload: ping.payload,
                };

                self.write_packet(pong).await?;

                Ok(())
            }
            _ => Err(anyhow!("Recieved a non status packet in the status stage!")),
        }
    }

    pub async fn handle_login(&mut self, packet: C2SPacket) -> Result<()> {
        use statik_proto::s2c::{
            login::{S2CDisconnect, S2CLoginSuccess, S2CSetCompression},
            play::{
                abilities, registry_bytes, void_chunk_bytes, S2CGameEvent, S2CLevelChunkWithLight,
                S2CLogin, S2CPlayerAbilities, S2CPlayerPosition, S2CSetChunkCacheCenter,
                S2CSetChunkCacheRadius, S2CSetDefaultSpawnPosition,
            },
        };
        use uuid::Uuid;

        match packet {
            C2SPacket::LoginStart(login_start) => {
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

                // Snapshot the config once — we need both limbo + compression
                // settings plus a couple of MC defaults.
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

                info!(
                    "Player \"{}\" (from {}) entering limbo at ({}, {}, {})",
                    login_start.username, self.address, limbo_pos[0], limbo_pos[1], limbo_pos[2]
                );

                // 1. Optional compression (must precede LoginSuccess).
                if compression_enabled {
                    self.write_packet(S2CSetCompression {
                        threshold: VarInt(compression_threshold),
                    })
                    .await?;
                    self.compression_threshold = Some(compression_threshold);
                }

                // 2. LoginSuccess — offline-mode UUID (nil). For a single-player limbo this is
                //    fine; for real server bridging we'd compute the offline-mode UUID v3 of
                //    the username.
                self.write_packet(S2CLoginSuccess {
                    uuid: Uuid::nil(),
                    username: login_start.username.clone(),
                    properties: vec![],
                })
                .await?;

                // 3. Transition to Play.
                self.state = State::Play;

                // 4. Initial Play burst. The Login Packet (0x28) MUST come first — it carries
                //    the dimension type / registry info the client uses to interpret the chunk
                //    packet that follows. Then chunk-cache setup, the empty chunk, spawn
                //    position, player position + abilities, and finally the Game Event that
                //    closes the "Loading Terrain" flow.
                let reg_bytes = registry_bytes().to_vec();
                debug!(
                    "BUILD S2CLogin: dimension={limbo_dimension:?} gamemode={limbo_gamemode} \
                     view={limbo_view} sim={limbo_sim} levels=[minecraft:the_void] \
                     registry_nbt={} bytes (first 16 = {:02x?})",
                    reg_bytes.len(),
                    &reg_bytes[..16.min(reg_bytes.len())]
                );
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
                // event = 12 = LEVEL_CHUNKS_LOAD_START. Sending this last
                // (after the chunk is loaded) lets the client exit the
                // "Loading Terrain" screen immediately.
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

            // statik does not implement encryption or login plugin negotiation
            // (it never sends EncryptionRequest / LoginPluginRequest). Receiving
            // either is a protocol violation — error out instead of panicking.
            C2SPacket::EncryptionResponse(_) => bail!(
                "Received EncryptionResponse from {} but statik never sends an EncryptionRequest; \
                 encryption is not supported.",
                self.address
            ),
            C2SPacket::LoginPluginResponse(_) => bail!(
                "Received LoginPluginResponse from {} but statik never sends a \
                 LoginPluginRequest; plugin login is not supported.",
                self.address
            ),
            other => bail!("Received a non-login packet in the login stage: {other:?}"),
        }
    }

    /// Handle a packet received while in Play state.
    ///
    /// In limbo we accept teleport acks silently, echo back the client's
    /// keepalive acks (the server also drives its own keepalive timer in
    /// [`handle_connection`]; the client's `C2SKeepAlive` is just the reply),
    /// and ignore the client's position updates — flying mode means the player
    /// won't fall, and the server doesn't actually care where they think they
    /// are.
    pub async fn handle_play(&mut self, packet: C2SPacket) -> Result<()> {
        match packet {
            C2SPacket::AcceptTeleportation(t) => {
                debug!("client accepted teleport id {:?}", t.id);
                Ok(())
            }
            C2SPacket::KeepAlive(k) => {
                // The client acked one of our `S2CKeepAlive`s. Nothing to do —
                // the keepalive timer in `handle_connection` drives the next
                // one. (We don't track pending ids in limbo; the ack is purely
                // a liveness signal.)
                trace!("client acked keepalive id {} from {}", k.id, self.address);
                Ok(())
            }
            C2SPacket::PlayerPos(_) | C2SPacket::PlayerPosRot(_) | C2SPacket::PlayerRot(_) => {
                // Client moved (or thinks it did). In limbo we don't act on
                // this — flying mode keeps them from falling, and we don't
                // enforce position server-side.
                trace!("ignored position update from {}", self.address);
                Ok(())
            }
            other => {
                trace!("ignored play packet: {other:?}");
                Ok(())
            }
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
        let pid = packet.id().0;

        // DEBUG: dump the S2CLogin (0x28) body to a file AND log a structural
        // walk of the NBT registry bytes so we can see exactly what bytes
        // hit the wire. Remove once limbo works.
        if pid == 0x28 {
            let dump_path = std::path::Path::new("statik_login_debug.bin");
            if let Err(e) = std::fs::write(dump_path, &self.staging) {
                warn!("could not write {}: {e}", dump_path.display());
            } else {
                debug!(
                    "SENT Login (id 0x28) body ({} bytes) dumped to {}",
                    self.staging.len(),
                    dump_path.display()
                );
            }
            // Walk the body and annotate the NBT portion (skip leading Login
            // packet fields: player_id, hardcore, game_type, previous_game_type,
            // levels VarInt, levels count, levels[0] length, levels[0] bytes,
            // registryHolder NBT).
            let body = &self.staging;
            debug!("SENT Login (0x28) body breakdown ({} bytes):", body.len());
            debug!("  [0]  packet_id 0x28");
            let mut off = 1;
            {
                let v =
                    i32::from_be_bytes([body[off], body[off + 1], body[off + 2], body[off + 3]]);
                debug!(
                    "  [{off:4}..{end:4}]  player_id i32={v}",
                    end = off + 4,
                    off = off
                );
                off += 4;
            }
            debug!("  [{off:4}]  hardcore = {}", body[off]);
            off += 1;
            if let Some((v, n)) = read_varint(body, off) {
                debug!(
                    "  [{off:4}..{end:4}]  game_type VarInt={v}",
                    end = off + n,
                    off = off
                );
                off += n;
            }
            if let Some((v, n)) = read_varint(body, off) {
                debug!(
                    "  [{off:4}..{end:4}]  previous_game_type VarInt={v}",
                    end = off + n,
                    off = off
                );
                off += n;
            }
            if let Some((count, n)) = read_varint(body, off) {
                debug!(
                    "  [{off:4}..{end:4}]  levels.count VarInt={count}",
                    end = off + n,
                    off = off
                );
                off += n;
                for i in 0..count {
                    if let Some((slen, n)) = read_varint(body, off) {
                        debug!(
                            "    levels[{i}] length VarInt={slen} at [{off:4}..{end:4}]",
                            end = off + n,
                            off = off
                        );
                        off += n;
                        let s = std::str::from_utf8(&body[off..off + slen as usize])
                            .unwrap_or("<bad utf-8>");
                        debug!(
                            "    levels[{i}] = {s:?} ({} bytes at [{off:4}..{end:4}])",
                            slen,
                            end = off + slen as usize
                        );
                        off += slen as usize;
                    }
                }
            }
            // registryHolder NBT starts here.
            debug!("  [{off:4}..]  registryHolder NBT:");
            walk_nbt(body, off, 0);
        } else {
            debug!(
                "(↑) sent packet 0x{pid:02x} ({} framed bytes)",
                framed.len()
            );
        }

        self.stream.write_all(&len_buf[..len_bytes]).await?;
        self.stream.write_all(&framed).await?;
        self.stream.flush().await?;

        Ok(())
    }
}

/// Read a VarInt from `buf` starting at `off`. Returns `(value,
/// bytes_consumed)`.
fn read_varint(buf: &[u8], off: usize) -> Option<(i32, usize)> {
    let mut value: i32 = 0;
    let mut shift = 0u32;
    let mut i = off;
    loop {
        if i >= buf.len() {
            return None;
        }
        let b = buf[i];
        value |= ((b & 0x7f) as i32) << shift;
        i += 1;
        if b & 0x80 == 0 {
            return Some((value, i - off));
        }
        shift += 7;
        if shift > 35 {
            return None;
        }
    }
}

/// Read a u16 BE length-prefixed NBT string. Returns `(s, bytes_consumed)`.
fn read_nbt_string(buf: &[u8], off: usize) -> Option<(&str, usize)> {
    if off + 2 > buf.len() {
        return None;
    }
    let len = u16::from_be_bytes([buf[off], buf[off + 1]]) as usize;
    let start = off + 2;
    let end = start + len;
    if end > buf.len() {
        return None;
    }
    let s = std::str::from_utf8(&buf[start..end]).ok()?;
    Some((s, end - off))
}

/// Walk the NBT structure starting at `off`, emitting a `debug!` line per
/// tag with offset range and (for Strings) value. Indents by `depth` for
/// readability. The Mojang client expects root tag byte + u16 BE name, then
/// children, and inner field names use u16 BE too.
fn walk_nbt(buf: &[u8], mut off: usize, depth: usize) {
    let indent = "  ".repeat(depth + 2);
    loop {
        if off >= buf.len() {
            debug!("{indent}[{off:4}]  (EOF)");
            return;
        }
        let tag = buf[off];
        let start = off;
        off += 1;
        match tag {
            0x00 => {
                debug!("{indent}[{start:4}]  TAG_End");
                return;
            }
            0x01 => {
                if let Some((name, n)) = read_nbt_string(buf, off) {
                    off += n;
                    if off < buf.len() {
                        debug!(
                            "{indent}[{start:4}..{end:4}]  TAG_Byte name={name:?} value=0x{:02x}",
                            buf[off],
                            end = off + 1
                        );
                        off += 1;
                    } else {
                        debug!("{indent}[{start:4}]  TAG_Byte name={name:?} (truncated)");
                        return;
                    }
                } else {
                    debug!("{indent}[{start:4}]  TAG_Byte (truncated name)");
                    return;
                }
            }
            0x02 => {
                if let Some((name, n)) = read_nbt_string(buf, off) {
                    off += n;
                    if off + 2 <= buf.len() {
                        let v = i16::from_be_bytes([buf[off], buf[off + 1]]);
                        debug!(
                            "{indent}[{start:4}..{end:4}]  TAG_Short name={name:?} value={v}",
                            end = off + 2
                        );
                        off += 2;
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
            0x03 => {
                if let Some((name, n)) = read_nbt_string(buf, off) {
                    off += n;
                    if off + 4 <= buf.len() {
                        let v = i32::from_be_bytes([
                            buf[off],
                            buf[off + 1],
                            buf[off + 2],
                            buf[off + 3],
                        ]);
                        debug!(
                            "{indent}[{start:4}..{end:4}]  TAG_Int name={name:?} value={v}",
                            end = off + 4
                        );
                        off += 4;
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
            0x04 => {
                if let Some((name, n)) = read_nbt_string(buf, off) {
                    off += n;
                    if off + 8 <= buf.len() {
                        let v = i64::from_be_bytes(buf[off..off + 8].try_into().unwrap());
                        debug!(
                            "{indent}[{start:4}..{end:4}]  TAG_Long name={name:?} value={v}",
                            end = off + 8
                        );
                        off += 8;
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
            0x05 => {
                if let Some((name, n)) = read_nbt_string(buf, off) {
                    off += n;
                    if off + 4 <= buf.len() {
                        let v = f32::from_be_bytes([
                            buf[off],
                            buf[off + 1],
                            buf[off + 2],
                            buf[off + 3],
                        ]);
                        debug!(
                            "{indent}[{start:4}..{end:4}]  TAG_Float name={name:?} value={v}",
                            end = off + 4
                        );
                        off += 4;
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
            0x06 => {
                if let Some((name, n)) = read_nbt_string(buf, off) {
                    off += n;
                    if off + 8 <= buf.len() {
                        let v = f64::from_be_bytes(buf[off..off + 8].try_into().unwrap());
                        debug!(
                            "{indent}[{start:4}..{end:4}]  TAG_Double name={name:?} value={v}",
                            end = off + 8
                        );
                        off += 8;
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
            0x07 => {
                if let Some((name, n)) = read_nbt_string(buf, off) {
                    off += n;
                    if off + 4 <= buf.len() {
                        let len = i32::from_be_bytes([
                            buf[off],
                            buf[off + 1],
                            buf[off + 2],
                            buf[off + 3],
                        ]) as usize;
                        off += 4;
                        if off + len <= buf.len() {
                            let preview = &buf[off..off + len.min(16)];
                            debug!(
                                "{indent}[{start:4}..{end:4}]  TAG_ByteArray name={name:?} \
                                 len={len} first16={:02x?}",
                                preview,
                                end = off + len
                            );
                            off += len;
                        } else {
                            return;
                        }
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
            0x08 => {
                if let Some((name, n)) = read_nbt_string(buf, off) {
                    off += n;
                    if let Some((s, m)) = read_nbt_string(buf, off) {
                        off += m;
                        debug!(
                            "{indent}[{start:4}..{end:4}]  TAG_String name={name:?} value={s:?}",
                            end = off
                        );
                    } else {
                        debug!("{indent}[{start:4}]  TAG_String name={name:?} (truncated value)");
                        return;
                    }
                } else {
                    return;
                }
            }
            0x09 => {
                if let Some((name, n)) = read_nbt_string(buf, off) {
                    off += n;
                    if off < buf.len() {
                        let elem_type = buf[off];
                        off += 1;
                        // List element count is i32 BE (Mojang uses
                        // `DataOutput.writeInt`), NOT VarInt.
                        if off + 4 <= buf.len() {
                            let count = i32::from_be_bytes([
                                buf[off],
                                buf[off + 1],
                                buf[off + 2],
                                buf[off + 3],
                            ]);
                            off += 4;
                            debug!(
                                "{indent}[{start:4}..]  TAG_List name={name:?} \
                                 element_tag=0x{elem_type:02x} count={count}"
                            );
                            for i in 0..count {
                                debug!("{indent}  element {i}:");
                                if elem_type == 0x0a {
                                    // List element is a Compound with no
                                    // leading tag byte; we read the name
                                    // directly and recurse into the body.
                                    if let Some((_ename, en)) = read_nbt_string(buf, off) {
                                        off += en;
                                    }
                                    walk_nbt(buf, off, depth + 2);
                                }
                            }
                        } else {
                            return;
                        }
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
            0x0a => {
                if let Some((name, n)) = read_nbt_string(buf, off) {
                    off += n;
                    debug!("{indent}[{start:4}..]  TAG_Compound name={name:?}");
                    walk_nbt(buf, off, depth + 1);
                    return;
                } else {
                    return;
                }
            }
            0x0b => {
                if let Some((name, n)) = read_nbt_string(buf, off) {
                    off += n;
                    if off + 4 <= buf.len() {
                        let len = i32::from_be_bytes([
                            buf[off],
                            buf[off + 1],
                            buf[off + 2],
                            buf[off + 3],
                        ]) as usize;
                        off += 4;
                        if off + len * 4 <= buf.len() {
                            debug!(
                                "{indent}[{start:4}..{end:4}]  TAG_IntArray name={name:?} \
                                 len={len}",
                                end = off + len * 4
                            );
                            off += len * 4;
                        } else {
                            return;
                        }
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
            0x0c => {
                if let Some((name, n)) = read_nbt_string(buf, off) {
                    off += n;
                    if off + 4 <= buf.len() {
                        let len = i32::from_be_bytes([
                            buf[off],
                            buf[off + 1],
                            buf[off + 2],
                            buf[off + 3],
                        ]) as usize;
                        off += 4;
                        if off + len * 8 <= buf.len() {
                            debug!(
                                "{indent}[{start:4}..{end:4}]  TAG_LongArray name={name:?} \
                                 len={len}",
                                end = off + len * 8
                            );
                            off += len * 8;
                        } else {
                            return;
                        }
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
            other => {
                debug!("{indent}[{start:4}]  unknown tag 0x{other:02x} — aborting walk");
                return;
            }
        }
    }
}
