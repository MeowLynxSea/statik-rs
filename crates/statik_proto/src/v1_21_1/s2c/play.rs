//! Server-to-client packets in the Play state (1.21.1, protocol 767).
//!
//! Field structure of the limbo-bound packets (GameEvent, KeepAlive,
//! PlayerPosition, PlayerAbilities, LevelChunkWithLight, SetChunkCacheCenter,
//! SetChunkCacheRadius, SetDefaultSpawnPosition) is the canonical 1.21.1
//! shape. The [`S2CLogin`] packet's registry arrives during Configuration
//! via `S2CRegistryData`; `gameType` / `previousGameType` live in the
//! inner [`SpawnInfo`] sub-struct ("CommonPlayerSpawnInfo" in Mojang
//! source), and new fields (`do_limited_crafting`, `enforces_secure_chat`,
//! `portal_cooldown`) appear. The full field layout is captured below from
//! PrismarineJS protocol.json (`play.toClient.packet_login` + the
//! `SpawnInfo` type).

use statik_core::prelude::*;
use statik_derive::*;

/// 0x1A - Disconnect.
#[derive(Debug, Packet)]
#[packet(id = 0x1A, state = State::Play)]
pub struct S2CDisconnectPlay {
    pub reason: Chat,
}

/// 0x22 - Game Event.
///
/// `event = 7` (START_WAITING_FOR_LEVELS) is sent on the wire as a
/// `VarInt`-sized integer (Mojang serialises the enum via
/// `FriendlyByteBuf.writeVarInt`).
#[derive(Debug, Packet)]
#[packet(id = 0x22, state = State::Play)]
pub struct S2CGameEvent {
    pub event: u8,
    pub param: f32,
}

/// 0x26 - Keep Alive.
#[derive(Debug, Packet)]
#[packet(id = 0x26, state = State::Play)]
pub struct S2CKeepAlive {
    pub id: i64,
}

/// 0x27 - Level Chunk With Light.
///
/// The precomputed body is built once in
/// [`crate::common::void_chunk_bytes_v1_21_1`] and re-exported below.
/// The 1.21.1 heightmaps field is an `anonymousNbt` (no u16 root-name
/// prefix) — see the re-export docs.
#[derive(Debug, Packet)]
#[packet(id = 0x27, state = State::Play)]
pub struct S2CLevelChunkWithLight {
    pub payload: RawBytes,
}

/// 0x2B - Login.
///
/// Full field layout (verified against PrismarineJS
/// `tmp/minecraft-data/data/pc/1.21.1/protocol.json`,
/// `play.toClient.packet_login` + `SpawnInfo`):
///
/// 1. `entity_id: i32`
/// 2. `is_hardcore: bool`
/// 3. `world_names: Vec<String>` (VarInt-counted)
/// 4. `max_players: VarInt`
/// 5. `view_distance: VarInt`
/// 6. `simulation_distance: VarInt`
/// 7. `reduced_debug_info: bool`
/// 8. `enable_respawn_screen: bool`
/// 9. `do_limited_crafting: bool`
/// 10. `world_state: SpawnInfo` (a nested container; see [`SpawnInfo`])
/// 11. `enforces_secure_chat: bool`
///
/// Registries arrive during Configuration via `S2CRegistryData` packets.
#[derive(Debug, Packet)]
#[packet(id = 0x2B, state = State::Play)]
pub struct S2CLogin {
    pub entity_id: i32,
    pub is_hardcore: bool,
    pub world_names: Vec<String>,
    pub max_players: VarInt,
    pub view_distance: VarInt,
    pub simulation_distance: VarInt,
    pub reduced_debug_info: bool,
    pub enable_respawn_screen: bool,
    pub do_limited_crafting: bool,
    pub world_state: SpawnInfo,
    pub enforces_secure_chat: bool,
}

/// "CommonPlayerSpawnInfo" — the nested `world_state` of [`S2CLogin`] and
/// also used in `S2CRespawn`.
///
/// Field layout per PrismarineJS protocol.json `SpawnInfo`:
/// - `dimension: VarInt` — index into the world's dimension-type registry
/// - `name: String` — dimension id (e.g. `"minecraft:the_void"`)
/// - `hashed_seed: i64`
/// - `gamemode: i8` (signed; PrismarineJS maps 0=survival, 1=creative,
///   2=adventure, 3=spectator)
/// - `previous_gamemode: u8` (255 = "no previous gamemode")
/// - `is_debug: bool`
/// - `is_flat: bool`
/// - `death: Option<DeathLocation>` — bool prefix + dimension + position
/// - `portal_cooldown: VarInt`
#[derive(Debug, Encode, Decode)]
pub struct SpawnInfo {
    pub dimension: VarInt,
    pub name: String,
    pub hashed_seed: i64,
    pub gamemode: i8,
    pub previous_gamemode: u8,
    pub is_debug: bool,
    pub is_flat: bool,
    pub death: Option<DeathLocation>,
    pub portal_cooldown: VarInt,
}

/// Last-death marker inside [`SpawnInfo`].
///
/// Only present when the client is being respawned at a non-default
/// location; for the initial Login burst this is `None`.
#[derive(Debug, Encode, Decode)]
pub struct DeathLocation {
    pub dimension_name: String,
    pub location: BlockPos,
}

/// 0x38 - Player Abilities.
///
/// `flags` is a single-byte bitfield: bit 0 invulnerable, bit 1 flying,
/// bit 2 can_fly, bit 3 instabuild. Use [`crate::common::abilities`] for
/// the constants.
#[derive(Debug, Packet)]
#[packet(id = 0x38, state = State::Play)]
pub struct S2CPlayerAbilities {
    pub flags: u8,
    pub flying_speed: f32,
    pub walking_speed: f32,
}

/// 0x40 - Synchronize Player Position.
///
/// Sent once on login with `relative_arguments = 0` (absolute teleport).
#[derive(Debug, Packet)]
#[packet(id = 0x40, state = State::Play)]
pub struct S2CPlayerPosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub y_rot: f32,
    pub x_rot: f32,
    pub relative_arguments: u8,
    pub id: VarInt,
}

/// 0x54 - Set Chunk Cache Center.
#[derive(Debug, Packet)]
#[packet(id = 0x54, state = State::Play)]
pub struct S2CSetChunkCacheCenter {
    pub x: VarInt,
    pub z: VarInt,
}

/// 0x55 - Set Chunk Cache Radius.
#[derive(Debug, Packet)]
#[packet(id = 0x55, state = State::Play)]
pub struct S2CSetChunkCacheRadius {
    pub radius: VarInt,
}

/// 0x56 - Set Default Spawn Position.
#[derive(Debug, Packet)]
#[packet(id = 0x56, state = State::Play)]
pub struct S2CSetDefaultSpawnPosition {
    pub location: BlockPos,
    pub angle: f32,
}

// == Re-exports == \\

/// Re-export of the 1.21.1-specific void-chunk payload builder. The
/// 1.21.1 heightmaps field is an `anonymousNbt` (no u16 root-name
/// prefix); using the wrong (named-NBT) builder makes the 1.21.1
/// client fail decoding the packet with
/// `Failed to decode packet 'clientbound/minecraft:level_chunk_with_light'`.
/// See [`crate::common::void_chunk_bytes_v1_21_1`] for the full wire
/// layout.
pub use crate::common::void_chunk_bytes_v1_21_1;
