//! Server-to-client packets for the Play state (protocol 763, MC 1.20.1).
//!
//! All the packets statik sends while a client is in Play state. The
//! `RegistryHolder` (in [`S2CLogin`]) and [`S2CLevelChunkWithLight`] payloads
//! are large, complex NBT / chunk-encoded blobs; they are constructed once
//! on first use and exposed via the [`void_chunk_bytes`] /
//! [`registry_bytes`] helpers (which return cached `&'static [u8]` slices).

use statik_core::prelude::*;
use statik_derive::*;

/// 0x1A - Disconnect Packet (S ➔ C, Play state).
///
/// Sent during Play state when we want to terminate the connection
/// gracefully (e.g. on shutdown, or to kick a misbehaving client).
#[derive(Debug, Packet)]
#[packet(id = 0x1A, state = State::Play)]
pub struct S2CDisconnectPlay {
    pub reason: Chat,
}

/// 0x1F - Game Event Packet (S ➔ C).
///
/// Sent with `event = 7` (`START_WAITING_FOR_LEVELS`) after the initial chunk
/// to signal the client to exit the "Loading Terrain" screen.
#[derive(Debug, Packet)]
#[packet(id = 0x1F, state = State::Play)]
pub struct S2CGameEvent {
    pub event: u8,
    pub param: f32,
}

/// 0x23 - Keep Alive Packet (S ➔ C, Play state).
///
/// Used for response-driven keepalive: when the client sends back our id,
/// we send a new one with an incremented value.
#[derive(Debug, Packet)]
#[packet(id = 0x23, state = State::Play)]
pub struct S2CKeepAlive {
    pub id: i64,
}

/// 0x24 - Level Chunk With Light Packet (S ➔ C).
///
/// Sends a single empty air chunk at (0,0) to satisfy the client's
/// `ClientChunkCache.hasAnyChunk()` and bypass the "Loading Terrain" screen.
/// The full body (chunk coordinates + `ClientboundLevelChunkPacketData` +
/// `ClientboundLightUpdatePacketData`) is built once by [`void_chunk_bytes`]
/// and stored as raw bytes in this `RawBytes` field.
#[derive(Debug, Packet)]
#[packet(id = 0x24, state = State::Play)]
pub struct S2CLevelChunkWithLight {
    pub payload: RawBytes,
}

/// 0x28 - Login Packet (S ➔ C).
///
/// The most complex packet in the protocol — fields after the `Vec<String>
/// levels` are mostly NBT and chunk-encoded structures. The `registry_holder`
/// field holds the entire game registry (dimension types, biomes, chat
/// types, etc.) and is built once by [`registry_bytes`].
///
/// Wire fields (per `tmp/mc-protocol-readmes/readme-1.20.1.md`):
/// `playerId(i32), hardcore(bool), gameType(VarInt),
/// previousGameType(VarInt), levels(Vec<String>), registryHolder(NBT),
/// dimensionType(String), dimension(String), seed(i64),
/// maxPlayers(VarInt), chunkRadius(VarInt), simulationDistance(VarInt),
/// reducedDebugInfo(bool), showDeathScreen(bool), isDebug(bool),
/// isFlat(bool), lastDeathLocation(Option<RawBytes>), portalCooldown(VarInt)`.
#[derive(Debug, Packet)]
#[packet(id = 0x28, state = State::Play)]
pub struct S2CLogin {
    pub player_id: i32,
    pub hardcore: bool,
    pub game_type: VarInt,
    pub previous_game_type: VarInt,
    pub levels: Vec<String>,
    pub registry_holder: RawBytes,
    pub dimension_type: String,
    pub dimension: String,
    pub seed: i64,
    pub max_players: VarInt,
    pub chunk_radius: VarInt,
    pub simulation_distance: VarInt,
    pub reduced_debug_info: bool,
    pub show_death_screen: bool,
    pub is_debug: bool,
    pub is_flat: bool,
    pub last_death_location: Option<RawBytes>,
    pub portal_cooldown: VarInt,
}

/// 0x34 - Player Abilities Packet (S ➔ C).
///
/// We send this once on login with flying + can_fly + invulnerable set to
/// grant the client permission to fly (preventing fall-damage in the void).
///
/// `flags` is a single byte bitfield (NOT separate bools):
/// `0x01` invulnerable, `0x02` flying, `0x04` can_fly, `0x08` instabuild.
/// Use the [`abilities`] consts to build it.
#[derive(Debug, Packet)]
#[packet(id = 0x34, state = State::Play)]
pub struct S2CPlayerAbilities {
    pub flags: u8,
    pub flying_speed: f32,
    pub walking_speed: f32,
}

/// Bit flags for [`S2CPlayerAbilities::flags`].
///
/// Re-exported from [`crate::common::abilities`] (shared across versions — the
/// single-byte bitfield format is unchanged 1.20.1 → 1.21.1).
pub use crate::common::abilities;

/// 0x3C - Synchronize Player Position Packet (S ➔ C).
///
/// Sent once on login with `relative_arguments = 0` (absolute teleport) to
/// place the client at the configured spawn position. The `id` is the
/// teleport id the client must echo back via `C2SAcceptTeleportation`.
///
/// `relative_arguments` is a single-byte bitfield (`0x01` X, `0x02` Y,
/// `0x04` Z, `0x08` Y_ROT, `0x10` X_ROT) — NOT a `BitSet`. An empty `BitSet`
/// happened to encode as a single `0x00` byte, matching `flags = 0`, but any
/// non-zero value would have mis-serialized.
#[derive(Debug, Packet)]
#[packet(id = 0x3C, state = State::Play)]
pub struct S2CPlayerPosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub y_rot: f32,
    pub x_rot: f32,
    pub relative_arguments: u8,
    pub id: VarInt,
}

/// 0x4E - Set Chunk Cache Center Packet (S ➔ C).
///
/// Sent on login to tell the client which chunk is the center of its view
/// (chunk coords are integers, not blocks).
#[derive(Debug, Packet)]
#[packet(id = 0x4E, state = State::Play)]
pub struct S2CSetChunkCacheCenter {
    pub x: VarInt,
    pub z: VarInt,
}

/// 0x4F - Set Chunk Cache Radius Packet (S ➔ C).
///
/// Sent on login to tell the client how many chunks around the center it
/// should keep loaded. Matches `LimboConfig::view_distance`.
#[derive(Debug, Packet)]
#[packet(id = 0x4F, state = State::Play)]
pub struct S2CSetChunkCacheRadius {
    pub radius: VarInt,
}

/// 0x50 - Set Default Spawn Position Packet (S ➔ C).
///
/// Sets the respawn anchor (used by the client for compass / death location
/// hints). `location` is a [`BlockPos`], encoded as Minecraft's packed `i64`.
#[derive(Debug, Packet)]
#[packet(id = 0x50, state = State::Play)]
pub struct S2CSetDefaultSpawnPosition {
    pub location: BlockPos,
    pub angle: f32,
}

// == Precomputed payloads == \\

/// Body of `S2CLevelChunkWithLight` for a single empty air chunk at (0,0).
///
/// The 1.20.1 and 1.21.1 chunk wire formats are identical (24-section paletted
/// container overworld), so this payload is built once in [`crate::common`]
/// and shared. See `common::void_chunk_bytes` for the wire layout.
pub use crate::common::void_chunk_bytes;

/// Body of the `registryHolder` field of `S2CLogin`: the complete vanilla
/// 1.20.1 `RegistryAccess$Frozen` network codec, embedded verbatim as NBT.
///
/// This is the canonical codec dumped from a real 1.20.1 server (via
/// PrismarineJS `minecraft-data`'s `loginPacket`), containing **all six**
/// registries the client requires: `minecraft:dimension_type`,
/// `minecraft:worldgen/biome`, `minecraft:chat_type`,
/// `minecraft:damage_type`, `minecraft:trim_pattern` and
/// `minecraft:trim_material`. An earlier hand-rolled subset omitted
/// `damage_type` (mandatory since 1.20), which made the client silently hang
/// on "Loading terrain" — the registry decode succeeded but building the
/// client `RegistryAccess` failed off the render thread.
///
/// The blob already has the network NBT framing (root `TAG_Compound` with a
/// zero-length name written as a u16), so it can be written straight into the
/// packet body via the `RawBytes` field.
pub fn registry_bytes() -> &'static [u8] {
    include_bytes!("../data/registry_1_20_1.nbt")
}
