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
pub mod abilities {
    pub const INVULNERABLE: u8 = 0x01;
    pub const FLYING: u8 = 0x02;
    pub const CAN_FLY: u8 = 0x04;
    pub const INSTABUILD: u8 = 0x08;
}

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
/// hints). `location` is a Minecraft `BlockPos` packed into a single `i64`
/// (NOT three `i32`s): `((x & 0x3FFFFFF) << 38) | ((z & 0x3FFFFFF) << 12) |
/// (y & 0xFFF)`. Use [`pack_block_pos`] to build it.
#[derive(Debug, Packet)]
#[packet(id = 0x50, state = State::Play)]
pub struct S2CSetDefaultSpawnPosition {
    pub location: i64,
    pub angle: f32,
}

/// Pack a block position into Minecraft's 64-bit `BlockPos` wire encoding:
/// `((x & 0x3FFFFFF) << 38) | ((z & 0x3FFFFFF) << 12) | (y & 0xFFF)`.
pub fn pack_block_pos(x: i32, y: i32, z: i32) -> i64 {
    ((x as i64 & 0x3FF_FFFF) << 38) | ((z as i64 & 0x3FF_FFFF) << 12) | (y as i64 & 0xFFF)
}

// == Precomputed payloads == \\

/// Body of `S2CLevelChunkWithLight` for a single empty air chunk at (0,0).
///
/// Wire layout (after the length-prefix and packet id):
/// - x(i32) = 0, z(i32) = 0
/// - `ClientboundLevelChunkPacketData`:
///   - heightmaps: NBT compound `{MOTION_BLOCKING: [i64; 256] zeros,
///     WORLD_SURFACE: [i64; 256] zeros}`
///   - chunk data buffer: 24 sections (overworld y range -64..320), each
///     section has `block_count=0`, single-value palette for block states (air
///     id 0), single-value palette for biomes, followed by 0 block entities.
/// - `ClientboundLightUpdatePacketData`: all four BitSets empty, no sky or
///   block light updates.
///
/// Constructed lazily on first call (computes ~2 KB of NBT + chunk sections).
pub fn void_chunk_bytes() -> &'static [u8] {
    chunk_payload::VOID_CHUNK
        .get_or_init(chunk_payload::build)
        .as_slice()
}

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
    include_bytes!("registry_1_20_1.nbt")
}

mod chunk_payload {

    use std::sync::OnceLock;

    /// Runtime-initialized empty chunk payload. Computed once on first use.
    pub(super) static VOID_CHUNK: OnceLock<Vec<u8>> = OnceLock::new();

    /// Build the empty chunk packet body. See `void_chunk_bytes` for layout.
    pub(super) fn build() -> Vec<u8> {
        let mut buf = Vec::with_capacity(2048);

        // x, z: chunk coordinates as i32 BE.
        buf.extend_from_slice(&0i32.to_be_bytes());
        buf.extend_from_slice(&0i32.to_be_bytes());

        // ClientboundLevelChunkPacketData:
        //   heightmaps: TAG_Compound "" { ... }
        buf.extend_from_slice(&heightmaps_nbt());
        //   size: VarInt count of bytes in `data`
        //   data: chunk sections
        let data_buf = chunk_data_buffer();
        write_var_int(&mut buf, data_buf.len() as i32);
        buf.extend_from_slice(&data_buf);
        //   block_entities: VarInt 0
        write_var_int(&mut buf, 0);

        // ClientboundLightUpdatePacketData:
        //   four BitSets (all empty -> single 0 byte each)
        buf.extend_from_slice(&[0u8; 4]);
        //   skyUpdates: VarInt 0 (no sections need sky light updates)
        write_var_int(&mut buf, 0);
        //   blockUpdates: VarInt 0
        write_var_int(&mut buf, 0);

        buf
    }

    /// Build the heightmaps NBT compound `{MOTION_BLOCKING: [i64; 256] zeros,
    /// WORLD_SURFACE: [i64; 256] zeros}`.
    ///
    /// Emits the **outer** compound: tag byte `0x0A`, then the root name "" —
    /// encoded as a **VarInt(0)** (one byte 0x00) because Mojang's
    /// `FriendlyByteBuf.writeNbt` writes the root name via `writeUtf` (VarInt
    /// length). Inner field names inside compounds use `DataOutput.writeUTF`
    /// (u16 length) instead.
    fn heightmaps_nbt() -> Vec<u8> {
        let mut buf = Vec::with_capacity(2100);

        // Outer TAG_Compound header: tag byte + root name (u16 length = 0).
        // The client reads the root name with `DataInput.readUTF` (u16 BE),
        // NOT VarInt — see the matching note in `registry_payload::build`.
        buf.push(0x0a);
        buf.extend_from_slice(&0u16.to_be_bytes());

        // MOTION_BLOCKING: TAG_Long_Array (0x0C), name, length **i32 BE**,
        // data. (NB: NBT array lengths use i32, not VarInt — Mojang uses
        // `DataOutput.writeInt`.)
        buf.push(0x0c);
        write_nbt_string(&mut buf, "MOTION_BLOCKING");
        buf.extend_from_slice(&256i32.to_be_bytes());
        buf.extend_from_slice(&[0u8; 256 * 8]);

        // WORLD_SURFACE: same shape.
        buf.push(0x0c);
        write_nbt_string(&mut buf, "WORLD_SURFACE");
        buf.extend_from_slice(&256i32.to_be_bytes());
        buf.extend_from_slice(&[0u8; 256 * 8]);

        // TAG_End.
        buf.push(0x00);

        buf
    }

    /// Build the chunk-data buffer: just the concatenated sections, with **no**
    /// leading section count. In 1.20.1 `ClientboundLevelChunkPacketData.write`
    /// serializes the sections back-to-back into the data buffer; the number of
    /// sections is implicit from the dimension height (the client reads exactly
    /// `levelHeightAccessor.getSectionsCount()` of them). For overworld
    /// (y = -64..320) there are 24 sections. (An earlier spurious
    /// `VarInt(num_sections)` prefix made the client mis-parse the first
    /// section's `block_count` and silently drop the chunk, leaving it stuck on
    /// "Loading terrain".)
    fn chunk_data_buffer() -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);
        for _ in 0..NUM_SECTIONS {
            write_section(&mut buf);
        }
        buf
    }

    /// Write a single all-air section. Layout (per `PalettedContainer.write`
    /// in Mojang 1.20.1):
    /// - `block_count`: i16 BE = 0
    /// - `block_states`: paletted container — bits_per_block = 0 (single
    ///   value), single value id = 0 (air), packed array length = 0 longs
    /// - `biomes`: paletted container — same shape with default biome id = 0
    fn write_section(buf: &mut Vec<u8>) {
        // block_count: 0 (i16 BE).
        buf.extend_from_slice(&0i16.to_be_bytes());

        // block_states: single-value palette.
        write_single_value_palette(buf);
        // biomes: single-value palette.
        write_single_value_palette(buf);
    }

    /// Encode a paletted container with a single value:
    /// - `bits_per_block`: u8 = 0 (Mojang writes this as a BYTE, not VarInt)
    /// - single value id: VarInt = 0 (for the singleton palette with size == 1,
    ///   Mojang writes only the id without a separate count)
    /// - packed array: VarInt(0) length, no long data
    fn write_single_value_palette(buf: &mut Vec<u8>) {
        buf.push(0u8); // bits per block as u8 (NOT VarInt)
        write_var_int(buf, 0); // single value id = 0 (air / default biome)
        write_var_int(buf, 0); // packed array length = 0 longs
    }

    fn write_var_int(buf: &mut Vec<u8>, mut value: i32) {
        loop {
            if (value & !0x7f) == 0 {
                buf.push(value as u8);
                return;
            }
            buf.push(((value & 0x7f) | 0x80) as u8);
            value = ((value as u32) >> 7) as i32;
        }
    }

    fn write_nbt_string(buf: &mut Vec<u8>, s: &str) {
        let bytes = s.as_bytes();
        buf.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(bytes);
    }

    /// Overworld has 24 sections (y = -64..320, 16 blocks tall each).
    const NUM_SECTIONS: i32 = 24;
}
