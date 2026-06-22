//! Client-to-server packets in the Configuration state (1.21.1+).
//!
//! Configuration is the post-`LoginSuccess` / pre-`Play` state introduced in
//! 1.20.2. statik uses it to negotiate features / known packs / registries
//! before transitioning the client into Play.

use statik_core::prelude::*;
use statik_derive::*;
use uuid::Uuid;

use crate::common::KnownPack;

/// 0x00 - Client Information (client settings).
///
/// `view_distance` is a single signed byte; `chat_mode` / `main_hand` are
/// `VarInt`; `skin_parts` is a `u8` bitmask. (In 1.20.1 this was a Play-state
/// packet; 1.21.1 moved it here.)
#[derive(Debug, Packet)]
#[packet(id = 0x00, state = State::Configuration)]
pub struct C2SClientInformation {
    pub locale: String,
    pub view_distance: i8,
    pub chat_mode: VarInt,
    pub chat_colors: bool,
    pub skin_parts: u8,
    pub main_hand: VarInt,
    pub text_filtering_enabled: bool,
    pub allows_listing: bool,
}

/// 0x01 - Cookie Response (Configuration).
#[derive(Debug, Packet)]
#[packet(id = 0x01, state = State::Configuration)]
pub struct C2SCookieResponse {
    pub key: String,
    pub payload: Option<RawBytes>,
}

/// 0x02 - Custom Payload (plugin message, e.g. `minecraft:brand`).
#[derive(Debug, Packet)]
#[packet(id = 0x02, state = State::Configuration)]
pub struct C2SConfigurationCustomPayload {
    pub channel: String,
    pub data: RawBytes,
}

/// 0x03 - Finish Configuration.
///
/// The client sends this when it is done negotiating configuration. statik
/// responds with `S2CFinishConfiguration` and transitions to Play.
#[derive(Debug, Packet)]
#[packet(id = 0x03, state = State::Configuration)]
/// _no fields._
pub struct C2SFinishConfiguration {}

/// 0x04 - Keep Alive (Configuration).
///
/// Note: Configuration C2S KeepAlive carries `id: i64`, **not** a VarInt —
/// this is the opposite of the S2C variant (which is `VarInt`).
#[derive(Debug, Packet)]
#[packet(id = 0x04, state = State::Configuration)]
pub struct C2SConfigurationKeepAlive {
    pub id: i64,
}

/// 0x05 - Pong (Configuration reply to a Configuration Ping).
#[derive(Debug, Packet)]
#[packet(id = 0x05, state = State::Configuration)]
pub struct C2SPongConfiguration {
    pub id: i32,
}

/// 0x06 - Resource Pack Response (uuid + result code).
///
/// Per PrismarineJS protocol.json the packet is `uuid: UUID + result:
/// varint` (Mojang's `ServerboundResourcePackPacket` carries the per-pack
/// UUID of the request that the response refers to). statik never sends
/// resource pack requests, so this packet is decoded for framing and
/// ignored.
#[derive(Debug, Packet)]
#[packet(id = 0x06, state = State::Configuration)]
pub struct C2SResourcePackResponse {
    pub uuid: Uuid,
    pub result: VarInt,
}

/// 0x07 - Select Known Packs.
///
/// The client lists the data packs it will use, drawn from the
/// `S2CKnownPacks` we sent. statik acks it and then sends `Finish
/// Configuration`.
#[derive(Debug, Packet)]
#[packet(id = 0x07, state = State::Configuration)]
pub struct C2SSelectKnownPacks {
    pub known_packs: Vec<KnownPack>,
}
