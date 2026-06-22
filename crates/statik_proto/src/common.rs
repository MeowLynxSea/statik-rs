//! Types and precomputed payloads shared across all Minecraft protocol
//! versions.
//!
//! This module holds anything that does **not** depend on a specific protocol
//! version:
//! - [`Property`] — a game-profile property, identical on the wire in every
//!   supported version (Login Success).
//! - [`KnownPack`] — a `KnownPack` entry used by the 1.21.1 Configuration
//!   `Known Packs` packets.
//! - [`abilities`] bit-flag constants for the S2C `PlayerAbilities` flags byte
//!   (the single-byte bitfield format is unchanged 1.20.1 → 1.21.1).
//! - [`void_chunk_bytes`] / `chunk_payload` — the empty-air-chunk payload for
//!   `S2CLevelChunkWithLight`. The 1.20.1 and 1.21.1 wire formats for this
//!   payload are identical (24-section paletted container overworld), so it is
//!   built once here and reused.

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

// == Precomputed void-chunk payload == \\

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
/// Constructed lazily on first call. Shared by every protocol version since
/// the 1.20.1 and 1.21.1 chunk wire formats are identical.
pub fn void_chunk_bytes() -> &'static [u8] {
    chunk_payload::VOID_CHUNK
        .get_or_init(chunk_payload::build)
        .as_slice()
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
