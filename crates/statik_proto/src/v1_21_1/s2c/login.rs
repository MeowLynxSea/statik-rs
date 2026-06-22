//! Server-to-client packets in the Login phase (1.21.1, protocol 767).

use statik_core::prelude::*;
use statik_derive::*;
use uuid::Uuid;

#[derive(Debug, Packet)]
#[packet(id = 0x00, state = State::Login)]
pub struct S2CDisconnect {
    pub reason: Chat,
}

#[derive(Debug, Packet)]
#[packet(id = 0x01, state = State::Login)]
pub struct S2CEncryptionRequest {
    pub server_id: String,
    pub public_key: Vec<u8>,
    pub verify_token: Vec<u8>,
}

/// Re-exported from [`crate::common::Property`] — wire format unchanged.
pub use crate::common::Property;

/// 0x02 - Login Success.
///
/// 1.21.1 adds a trailing `strict_error_handling: bool` field (see Mojang
/// `ClientboundGameProfilePacket`). PrismarineJS protocol.json (the
/// canonical reference for this project) lists exactly these four fields
/// in this order; verified against `tmp/minecraft-data/data/pc/1.21.1/
/// protocol.json`. statik sends `strict_error_handling = false` (lenient
/// mode — matches vanilla server behaviour).
#[derive(Debug, Packet)]
#[packet(id = 0x02, state = State::Login)]
pub struct S2CLoginSuccess {
    pub uuid: Uuid,
    pub username: String,
    pub properties: Vec<Property>,
    pub strict_error_handling: bool,
}

#[derive(Debug, Packet)]
#[packet(id = 0x03, state = State::Login)]
pub struct S2CSetCompression {
    pub threshold: VarInt,
}

#[derive(Debug, Packet)]
#[packet(id = 0x04, state = State::Login)]
pub struct S2CLoginPluginRequest {
    pub message_id: VarInt,
    pub channel: String,
    pub data: Vec<u8>,
}
