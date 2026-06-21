//! Client-to-server packets for the Play state (protocol 763, MC 1.20.1).
//!
//! These are the minimum packets statik decodes while a client is in Play
//! state. Anything not in this list is decoded as `C2SPacket`'s `Other`
//! variant (when added) or fails to decode — see `connection.rs::handle_play`.

use statik_core::prelude::*;
use statik_derive::*;

/// 0x00 - Accept Teleportation Packet (C ➔ S).
///
/// Sent by the client to acknowledge a `Synchronize Player Position` packet
/// from the server. The `id` must match the teleport id we sent; we don't
/// actually care — we just note the ack and move on.
#[derive(Debug, Packet)]
#[packet(id = 0x00, state = State::Play)]
pub struct C2SAcceptTeleportation {
    pub id: VarInt,
}

/// 0x08 - Client Information Packet (C ➔ S).
///
/// Sent by the client immediately after entering Play state (and again
/// whenever the player changes their options). We don't act on it, but we
/// must decode it cleanly or the connection would otherwise be dropped the
/// instant the player joins limbo.
///
/// Wire format gotchas (readme's `Raw Type` lists Java field types, not wire
/// types — see CLAUDE.md): `viewDistance` is a single signed `byte` (NOT a
/// VarInt/int), `modelCustomisation` is an unsigned `byte` bitmask, and
/// `chatVisibility` / `mainHand` are enums serialized as `VarInt`.
#[derive(Debug, Packet)]
#[packet(id = 0x08, state = State::Play)]
pub struct C2SClientInformation {
    pub language: String,
    pub view_distance: i8,
    pub chat_visibility: VarInt,
    pub chat_colors: bool,
    pub model_customisation: u8,
    pub main_hand: VarInt,
    pub text_filtering_enabled: bool,
    pub allows_listing: bool,
}

/// 0x12 - Keep Alive Packet (C ➔ S).
///
/// The client echoes the `id` we sent in `S2CKeepAlive`. We respond with a
/// fresh keep-alive of our own (response-driven — avoids needing a timer).
#[derive(Debug, Packet)]
#[packet(id = 0x12, state = State::Play)]
pub struct C2SKeepAlive {
    pub id: i64,
}

/// 0x14 - Move Player Pos Packet (C ➔ S).
///
/// Sent by the client whenever it moves and only position changed. In limbo
/// the client thinks it can move (especially in creative + flying mode) and
/// we ignore these. The `has_pos` / `has_rot` flags distinguish the three
/// "Move Player" variants which share id space in 1.20.1.
#[derive(Debug, Packet)]
#[packet(id = 0x14, state = State::Play)]
pub struct C2SPlayerPos {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub y_rot: f32,
    pub x_rot: f32,
    pub on_ground: bool,
    pub has_pos: bool,
    pub has_rot: bool,
}

/// 0x15 - Move Player Pos Rot Packet (C ➔ S).
#[derive(Debug, Packet)]
#[packet(id = 0x15, state = State::Play)]
pub struct C2SPlayerPosRot {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub y_rot: f32,
    pub x_rot: f32,
    pub on_ground: bool,
    pub has_pos: bool,
    pub has_rot: bool,
}

/// 0x16 - Move Player Rot Packet (C ➔ S).
#[derive(Debug, Packet)]
#[packet(id = 0x16, state = State::Play)]
pub struct C2SPlayerRot {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub y_rot: f32,
    pub x_rot: f32,
    pub on_ground: bool,
    pub has_pos: bool,
    pub has_rot: bool,
}
