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

/// 0x08 - Chunk Batch Received.
///
/// Client's ack for the server's `S2CChunkBatchFinished`. Introduced in
/// 1.20.2+ alongside the chunk batching protocol; `desired_chunks_per_tick`
/// is the client's hint to the server for the next batch's chunk rate.
/// statik ignores the value (limbo sends exactly one chunk per
/// connection) but models the packet to keep `decode_in_state` from
/// falling into the "ignoring undecodable Play packet" debug branch.
#[derive(Debug, Packet)]
#[packet(id = 0x08, state = State::Play)]
pub struct C2SChunkBatchReceived {
    pub desired_chunks_per_tick: f32,
}

/// 0x18 - Keep Alive.
///
/// `id` is 8-byte signed BE (i64).
#[derive(Debug, Packet)]
#[packet(id = 0x18, state = State::Play)]
pub struct C2SKeepAlive {
    pub id: i64,
}

/// 0x1A - Move Player Pos.
#[derive(Debug, Packet)]
#[packet(id = 0x1A, state = State::Play)]
pub struct C2SPlayerPos {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub on_ground: bool,
}

/// 0x1B - Move Player Pos Rot.
#[derive(Debug, Packet)]
#[packet(id = 0x1B, state = State::Play)]
pub struct C2SPlayerPosRot {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub y_rot: f32,
    pub x_rot: f32,
    pub on_ground: bool,
}

/// 0x1C - Move Player Rot.
#[derive(Debug, Packet)]
#[packet(id = 0x1C, state = State::Play)]
pub struct C2SPlayerRot {
    pub y_rot: f32,
    pub x_rot: f32,
    pub on_ground: bool,
}

/// 0x1D - Move Player Status Only.
///
/// The Mojang class `ServerboundMovePlayerPacket` unifies Pos / PosRot /
/// Rot / StatusOnly under one parent with `hasPos` / `hasRot` booleans, but
/// the wire format still emits only the subclass-specific fields — no extra
/// bytes on the wire.
#[derive(Debug, Packet)]
#[packet(id = 0x1D, state = State::Play)]
pub struct C2SPlayerFlying {
    pub on_ground: bool,
}
