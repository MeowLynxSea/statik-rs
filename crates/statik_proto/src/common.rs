//! Types and precomputed payloads shared across all Minecraft protocol
//! versions.
//!
//! This module holds anything that does **not** depend on a specific protocol
//! version:
//! - [`Property`] — a game-profile property, identical on the wire in every
//!   supported version (Login Success).
//! - [`KnownPack`] — a `KnownPack` entry used by the 1.21.1 Configuration
//!   `Known Packs` packets.
//! - [`Tag`], [`TagGroup`] — wire types for the Configuration `Update Tags`
//!   packet (S2C 0x0D). The format is identical across 1.20.2+, so the types
//!   live here and are reused by every per-protocol-version `S2CUpdateTags`
//!   packet rather than being re-declared in each `v1_XX` module.
//! - [`abilities`] bit-flag constants for the S2C `PlayerAbilities` flags byte
//!   (the single-byte bitfield format is unchanged 1.20.1 → 1.21.1).
//! - [`void_chunk_bytes_v1_20_1`] / [`void_chunk_bytes_v1_21_1`] — the
//!   empty-air-chunk payload for `S2CLevelChunkWithLight`. The chunk-section
//!   data and light-update framing are wire-equivalent between 1.20.1 and
//!   1.21.1, but the **heightmaps NBT** is different: 1.20.1 writes a
//!   TAG_Compound with a u16 length=0 root name (named NBT) while 1.21.1 writes
//!   it anonymously (no root name). The version-specific entry points are
//!   exposed so each `v1_XX::s2c::play` module can pick the right one.

use statik_core::varint::VarInt;
use statik_derive::*;

/// One signed property entry on a game profile (Login Success `properties`).
///
/// Identical on the wire in 1.20.1 and 1.21.1. `signature` is `None` for
/// unsigned properties.
#[derive(Debug, Encode, Decode)]
pub struct Property {
    /// Property name, e.g. `"textures"`.
    pub name: String,
    /// Property value (base64-encoded payload).
    pub value: String,
    /// Optional base64 signature; absent for unsigned properties.
    pub signature: Option<String>,
}

/// A `KnownPack` entry, exchanged during the 1.21.1 Configuration state
/// (`Known Packs` S2C 0x0E / `Select Known Packs` C2S 0x07).
///
/// On the wire each entry is three `String`s (`namespace`, `id`, `version`),
/// prefixed by a `VarInt` count for the whole list.
#[derive(Debug, Encode, Decode, Clone)]
pub struct KnownPack {
    pub namespace: String,
    pub id: String,
    pub version: String,
}

/// One tag registry group (e.g. `"minecraft:block"`, `"minecraft:item"`) in
/// the Configuration `Update Tags` packet (S2C 0x0D).
///
/// Wire layout per PrismarineJS `packet_tags`: `tag_type: String`, then a
/// `VarInt` count of [`Tag`] entries. The format has been stable across
/// every Minecraft version that has the Configuration state (1.20.2+).
///
/// 1.20.1 has no Configuration state and therefore never sends `Update
/// Tags`. The types still live here (not in `v1_20_1`) because they are
/// per-version-packet-agnostic — every per-version `S2CUpdateTags` struct
/// reuses them, and adding a new protocol version only requires declaring
/// the new `S2CUpdateTags` packet id.
#[derive(Debug, Encode, Decode, Clone)]
pub struct TagGroup {
    /// Registry identifier, e.g. `"minecraft:block"`.
    pub tag_type: String,
    /// Tags defined for this registry.
    pub tags: Vec<Tag>,
}

/// One named tag — a list of registry-local entry ids.
///
/// `entries` are ids into the registry identified by the parent
/// [`TagGroup::tag_type`]. The id↔name mapping for those ids is established
/// by the corresponding `S2CRegistryData` packet sent just before
/// `S2CUpdateTags`.
#[derive(Debug, Encode, Decode, Clone)]
pub struct Tag {
    /// Tag identifier, e.g. `"minecraft:stone"`.
    pub name: String,
    /// VarInt-encoded registry-local ids.
    pub entries: Vec<VarInt>,
}

/// Bit flags for the S2C `PlayerAbilities` `flags` byte.
///
/// `flags` is a single byte bitfield (NOT separate bools):
/// `0x01` invulnerable, `0x02` flying, `0x04` can_fly, `0x08` instabuild.
/// This format is unchanged from 1.20.1 to 1.21.1.
pub mod abilities {
    pub const INVULNERABLE: u8 = 0x01;
    pub const FLYING: u8 = 0x02;
    pub const CAN_FLY: u8 = 0x04;
    pub const INSTABUILD: u8 = 0x08;
}

// == Precomputed void-chunk payloads == \\

/// Body of `S2CLevelChunkWithLight` (1.20.1) for a single empty air chunk
/// at (0,0).
///
/// Wire layout (after the length-prefix and packet id):
/// - x(i32) = 0, z(i32) = 0
/// - `ClientboundLevelChunkPacketData`:
///   - heightmaps: **named** NBT compound — `TAG_Compound(0x0A)` followed by a
///     u16 length=0 root name (the empty string), then the field entries
///     `{MOTION_BLOCKING: [i64; 256] zeros, WORLD_SURFACE: [i64; 256] zeros}`,
///     then `TAG_End(0x00)`. The u16 root-name length is what Mojang's
///     `DataInput.readUTF` reads.
///   - chunk data buffer: 24 sections (overworld y range -64..320), each
///     section has `block_count=0`, single-value palette for block states (air
///     id 0), single-value palette for biomes, followed by 0 block entities.
/// - `ClientboundLightUpdatePacketData`: all four BitSets empty, no sky or
///   block light updates.
///
/// Constructed lazily on first call. For 1.21.1 use
/// [`void_chunk_bytes_v1_21_1`] — the chunk-section and light-update
/// framing is identical, but the heightmaps NBT is written anonymously
/// (no u16 root name prefix), which is what the 1.21.1 client expects.
pub fn void_chunk_bytes_v1_20_1() -> &'static [u8] {
    chunk_payload::VOID_CHUNK_V1_20_1
        .get_or_init(chunk_payload::build_v1_20_1)
        .as_slice()
}

/// Body of `S2CLevelChunkWithLight` (1.21.1) for a single empty air chunk
/// at (0,0).
///
/// Wire layout (after the length-prefix and packet id):
/// - x(i32) = 0, z(i32) = 0
/// - `ClientboundLevelChunkPacketData`:
///   - heightmaps: **anonymous** NBT compound — `TAG_Compound(0x0A)` with
///     **no** root-name prefix, then the field entries `{MOTION_BLOCKING: [i64;
///     256] zeros, WORLD_SURFACE: [i64; 256] zeros}`, then `TAG_End(0x00)`. The
///     1.20.1 format prefixes this with a u16 length=0 root name; the 1.21.1
///     client rejects that, so writing the 1.20.1 payload to a 1.21.1 client
///     produces `Failed to decode packet
///     'clientbound/minecraft:level_chunk_with_light'`.
///   - chunk data buffer: 24 sections (same encoding as 1.20.1), followed by 0
///     block entities.
/// - `ClientboundLightUpdatePacketData`: four empty `i64[]varint` masks and two
///   empty `u8[]varint` light-update arrays (functionally identical to 1.20.1's
///   BitSet + array-of-LightData shape — both serialise as a `VarInt(0)` per
///   field).
///
/// Constructed lazily on first call.
pub fn void_chunk_bytes_v1_21_1() -> &'static [u8] {
    chunk_payload::VOID_CHUNK_V1_21_1
        .get_or_init(chunk_payload::build_v1_21_1)
        .as_slice()
}

mod chunk_payload {

    use std::sync::OnceLock;

    /// Runtime-initialized empty chunk payload for 1.20.1. Computed once
    /// on first use.
    pub(super) static VOID_CHUNK_V1_20_1: OnceLock<Vec<u8>> = OnceLock::new();

    /// Runtime-initialized empty chunk payload for 1.21.1. Computed once
    /// on first use.
    pub(super) static VOID_CHUNK_V1_21_1: OnceLock<Vec<u8>> = OnceLock::new();

    /// Build the empty chunk packet body for 1.20.1. See
    /// [`crate::common::void_chunk_bytes_v1_20_1`] for layout.
    pub(super) fn build_v1_20_1() -> Vec<u8> {
        let mut buf = Vec::with_capacity(2048);
        append_common_chunk(&mut buf, &heightmaps_nbt_v1_20_1());
        buf
    }

    /// Build the empty chunk packet body for 1.21.1. See
    /// [`crate::common::void_chunk_bytes_v1_21_1`] for layout.
    pub(super) fn build_v1_21_1() -> Vec<u8> {
        let mut buf = Vec::with_capacity(2048);
        append_common_chunk(&mut buf, &heightmaps_nbt_v1_21_1());
        buf
    }

    /// Append everything past the packet-id VarInt: x, z, heightmaps NBT,
    /// chunk data buffer (size + 24 sections + block-entities count), and
    /// the four empty light masks + two empty light-update arrays.
    ///
    /// The chunk-section encoding and the light-update framing are
    /// wire-equivalent between 1.20.1 and 1.21.1, so only the heightmaps
    /// NBT is version-specific and is supplied as `heightmaps`.
    fn append_common_chunk(buf: &mut Vec<u8>, heightmaps: &[u8]) {
        // x, z: chunk coordinates as i32 BE.
        buf.extend_from_slice(&0i32.to_be_bytes());
        buf.extend_from_slice(&0i32.to_be_bytes());

        // Heightmaps NBT (format depends on `heightmaps`).
        buf.extend_from_slice(heightmaps);

        // ClientboundLevelChunkPacketData continues:
        //   size: VarInt count of bytes in `data`
        //   data: chunk sections
        let data_buf = chunk_data_buffer();
        write_var_int(buf, data_buf.len() as i32);
        buf.extend_from_slice(&data_buf);
        //   block_entities: VarInt 0
        write_var_int(buf, 0);

        // ClientboundLightUpdatePacketData:
        //   four BitSets (1.20.1) / four i64[]varint (1.21.1) — both
        //   serialise as VarInt(0) for an empty mask; the four single-byte
        //   zeros below are four VarInt(0) entries, not raw 4-byte BitSet
        //   structures.
        buf.extend_from_slice(&[0u8; 4]);
        //   skyUpdates (1.20.1) / skyLight outer count (1.21.1) = VarInt(0)
        write_var_int(buf, 0);
        //   blockUpdates (1.20.1) / blockLight outer count (1.21.1) = VarInt(0)
        write_var_int(buf, 0);
    }

    /// Build the 1.20.1 heightmaps NBT: a **named** TAG_Compound
    /// `{MOTION_BLOCKING: [i64; 256] zeros, WORLD_SURFACE: [i64; 256]
    /// zeros}`.
    ///
    /// Emits the outer compound as: tag byte `0x0A`, then a u16 length=0
    /// root name (the empty string), then the field entries, then
    /// `TAG_End(0x00)`. The 1.20.1 client reads the root name with
    /// `DataInput.readUTF` (u16 BE) and expects this prefix.
    fn heightmaps_nbt_v1_20_1() -> Vec<u8> {
        let mut buf = Vec::with_capacity(2100);

        // Outer TAG_Compound header: tag byte + u16 length=0 root name.
        buf.push(0x0a);
        buf.extend_from_slice(&0u16.to_be_bytes());

        // MOTION_BLOCKING: TAG_Long_Array (0x0C), name, i32 BE length, data.
        // (NBT array lengths use i32 BE — `DataOutput.writeInt`, not VarInt.)
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

    /// Build the 1.21.1 heightmaps NBT: an **anonymous** TAG_Compound
    /// `{MOTION_BLOCKING: [i64; 256] zeros, WORLD_SURFACE: [i64; 256]
    /// zeros}`.
    ///
    /// Emits the outer compound as: tag byte `0x0A`, then the field
    /// entries, then `TAG_End(0x00)`. **No** u16 root-name prefix — the
    /// 1.21.1 client reads the heightmaps with the equivalent of
    /// `DataInput.readByte` for the tag byte followed by direct field
    /// parsing (PrismarineJS models this as `anonymousNbt`).
    fn heightmaps_nbt_v1_21_1() -> Vec<u8> {
        let mut buf = Vec::with_capacity(2098);

        // Outer TAG_Compound header: tag byte only — NO root name.
        buf.push(0x0a);

        // MOTION_BLOCKING: TAG_Long_Array (0x0C), name, i32 BE length, data.
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
    ///
    /// Identical for 1.20.1 and 1.21.1.
    fn chunk_data_buffer() -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);
        for _ in 0..NUM_SECTIONS {
            write_section(&mut buf);
        }
        buf
    }

    /// Write a single all-air section. Layout (per `PalettedContainer.write`
    /// in Mojang 1.20.1; unchanged in 1.21.1 for empty chunks):
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
