use statik_core::prelude::*;
use statik_derive::*;

/// 0x00 - Handshake.
///
/// `next_state` / `intention` is a [`ClientIntent`]: `Status = 1`,
/// `Login = 2`, `Transfer = 3`. statik only handles `Status` and `Login`;
/// `Transfer` is rejected at the handshake handler.
#[derive(Debug, Packet)]
#[packet(id = 0x00, state = State::Handshake)]
pub struct C2SHandshake {
    /// 767 in Minecraft 1.21.1.
    pub protocol_version: VarInt,
    pub server_address: String,
    /// Wire: unsigned short BE.
    pub server_port: u16,
    /// 1 = Status, 2 = Login, 3 = Transfer.
    pub next_state: ClientIntent,
}
