use std::{
    io::{self, Cursor, ErrorKind},
    net::SocketAddr,
    sync::Arc,
};

use bytes::{Buf, BytesMut};
use statik_core::prelude::*;
use statik_proto::{
    c2s::C2SPacket,
    s2c::status::response::{Players, StatusResponse},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::TcpStream,
    sync::RwLock,
};

use crate::config::ServerConfig;

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
    /// # Returns
    ///
    /// Returns `Err` if the `TcpStream` is closed (mapped to
    /// [`io::ErrorKind::UnexpectedEof`]) or a malformed frame is encountered.
    /// Callers may treat a clean EOF as a normal connection end.
    pub async fn handle_connection(&mut self) -> Result<()> {
        loop {
            trace!("handling connection with {}", self.address);

            // Drain every complete frame currently buffered. `try_parse_packet`
            // returns `Ok(None)` when there is not enough data yet for the next
            // frame, in which case we fall through and read more bytes.
            while let Some(packet) = self.try_parse_packet()? {
                self.dispatch_packet(packet).await?;
            }

            let bytes_read = self.stream.read_buf(&mut self.buffer).await?;

            if bytes_read == 0 {
                return Err(io::Error::from(ErrorKind::UnexpectedEof).into());
            }

            trace!("read {bytes_read} bytes from {}.", self.address);
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

        let packet = {
            let mut body = Cursor::new(&self.buffer[..length]);
            C2SPacket::decode_in_state(self.state, &mut body)?
        };
        debug!("(↓) packet recieved: {:?}", &packet);

        self.buffer.advance(length);

        Ok(Some(packet))
    }

    /// Routes a decoded packet to the handler for the connection's current
    /// state.
    async fn dispatch_packet(&mut self, packet: C2SPacket) -> Result<()> {
        match self.state {
            State::Handshake => self.handle_handshake(packet).await,
            State::Status => self.handle_status(packet).await,
            State::Login => self.handle_login(packet).await,
            State::Play => unimplemented!(),
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
        use statik_proto::s2c::login::S2CDisconnect;
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

                // The disconnect message is config-driven. A `{username}`
                // placeholder is substituted with the connecting player's name
                // (full templating is a future TODO — see TODO.md).
                let disconnect_msg = {
                    let config = self.config.read().await;
                    config
                        .mc
                        .disconnect_msg
                        .replace("{username}", &login_start.username)
                };

                info!(
                    "Player \"{}\" (from {}) attempted login; disconnecting and signalling the \
                     real server to start.",
                    login_start.username, self.address
                );

                let disconnect = S2CDisconnect {
                    reason: Chat::new(disconnect_msg),
                };

                self.write_packet(disconnect).await?;

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

    /// Encodes `packet` and writes it to the stream, framed with a leading
    /// VarInt length prefix: `[VarInt(length), packet-id VarInt, fields...]`.
    ///
    /// The packet body is encoded into `staging` first so its length is known,
    /// then the length prefix (at most 5 bytes, encoded on the stack) and the
    /// body are written to the buffered stream and flushed.
    pub async fn write_packet(&mut self, packet: impl Packet) -> Result<()> {
        self.staging.clear();
        packet.encode(&mut self.staging)?;

        // VarInt is at most 5 bytes; encode the length prefix on the stack.
        let mut len_buf = [0u8; 5];
        let mut len_cursor = std::io::Cursor::new(&mut len_buf[..]);
        VarInt(self.staging.len() as i32).encode(&mut len_cursor)?;
        let len_bytes = len_cursor.position() as usize;

        trace!("writing packet to tcp stream: {packet:?}");
        self.stream.write_all(&len_buf[..len_bytes]).await?;
        self.stream.write_all(&self.staging).await?;
        self.stream.flush().await?;

        trace!("(↑) packet sent.");
        Ok(())
    }
}
