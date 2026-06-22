//! Client-to-server packets in the Play state (1.21.1, protocol 767).
//!
//! statik only **acts** on a handful of packets (teleport ack, keep-alive
//! ack, movement); all others are decoded for framing and silently ignored.

use statik_core::prelude::*;
use statik_derive::*;

/// 0x00 - Accept Teleportation (Teleport Confirm).
#[derive(Debug, Packet)]
#[packet(id = 0x00, state = State::Play)]
pub struct C2SAcceptTeleportation {
    pub id: VarInt,
}

/// 0x18 - Keep Alive.
///
/// `id` is 8-byte signed BE (i64). Note the id shifted from `0x12` in 1.20.1
/// to `0x18` in 1.21.1.
#[derive(Debug, Packet)]
#[packet(id = 0x18, state = State::Play)]
pub struct C2SKeepAlive {
    pub id: i64,
}

/// 0x14 - Move Player Pos.
#[derive(Debug, Packet)]
#[packet(id = 0x14, state = State::Play)]
pub struct C2SPlayerPos {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub on_ground: bool,
}

/// 0x15 - Move Player Pos Rot.
#[derive(Debug, Packet)]
#[packet(id = 0x15, state = State::Play)]
pub struct C2SPlayerPosRot {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub y_rot: f32,
    pub x_rot: f32,
    pub on_ground: bool,
}

/// 0x16 - Move Player Rot.
#[derive(Debug, Packet)]
#[packet(id = 0x16, state = State::Play)]
pub struct C2SPlayerRot {
    pub y_rot: f32,
    pub x_rot: f32,
    pub on_ground: bool,
}

/// 0x17 - Move Player Flying (renamed from `StatusOnly` in 1.20.1).
///
/// Wire format is identical: just `on_ground: bool`.
#[derive(Debug, Packet)]
#[packet(id = 0x17, state = State::Play)]
pub struct C2SPlayerFlying {
    pub on_ground: bool,
}
