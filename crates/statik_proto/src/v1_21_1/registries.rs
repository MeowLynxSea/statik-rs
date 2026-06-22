//! Precomputed 1.21.1 registry blobs for the Configuration `RegistryData`
//! packets.
//!
//! The blobs in `data/registry_*.bin` were generated from
//! `tmp/minecraft-data/data/pc/1.21.1/loginPacket.json` (PrismarineJS
//! `minecraft-data`) using `build_tools/build_registry_nbt.py`. Each blob is
//! the **content** of an `S2CRegistryData.data` field (i.e. the bytes that
//! follow the leading `registry_id` String on the wire):
//!
//! ```text
//! VarInt(entry_count)
//! for each entry:
//!     String(key)               // VarInt-length + UTF-8
//!     bool(value_present = 1)   // statik always sends present values
//!     anonymous_nbt_bytes       // 1.20.2+ no-outer-name NBT tag
//! ```
//!
//! Each helper function returns a `&'static [u8]` view of the embedded
//! blob, suitable for stuffing into an `S2CRegistryData { registry_id,
//! data: RawBytes(...) }` packet.

macro_rules! registry_blob {
    ($fn_name:ident, $registry_id:expr, $file:literal) => {
        #[doc = concat!("Precomputed `S2CRegistryData.data` for `", $registry_id, "`.")]
        pub fn $fn_name() -> &'static [u8] {
            include_bytes!(concat!("data/", $file))
        }
    };
}

registry_blob!(
    dimension_type,
    "minecraft:dimension_type",
    "registry_dimension_type.bin"
);
registry_blob!(
    worldgen_biome,
    "minecraft:worldgen/biome",
    "registry_worldgen_biome.bin"
);
registry_blob!(chat_type, "minecraft:chat_type", "registry_chat_type.bin");
registry_blob!(
    damage_type,
    "minecraft:damage_type",
    "registry_damage_type.bin"
);
registry_blob!(
    trim_pattern,
    "minecraft:trim_pattern",
    "registry_trim_pattern.bin"
);
registry_blob!(
    trim_material,
    "minecraft:trim_material",
    "registry_trim_material.bin"
);
registry_blob!(
    wolf_variant,
    "minecraft:wolf_variant",
    "registry_wolf_variant.bin"
);
registry_blob!(
    painting_variant,
    "minecraft:painting_variant",
    "registry_painting_variant.bin"
);
registry_blob!(
    banner_pattern,
    "minecraft:banner_pattern",
    "registry_banner_pattern.bin"
);
registry_blob!(
    enchantment,
    "minecraft:enchantment",
    "registry_enchantment.bin"
);
registry_blob!(
    jukebox_song,
    "minecraft:jukebox_song",
    "registry_jukebox_song.bin"
);

/// A `(registry_id, blob_getter)` entry. Returned by [`all`].
///
/// The blob getter is a function pointer so the `&'static` slice can be
/// constructed without a `Vec` allocation.
pub type RegistryEntry = (&'static str, fn() -> &'static [u8]);

/// The (id, blob) pairs that statik sends during the Configuration burst.
///
/// 1.21.1 vanilla clients require **all** of these registries to be
/// present before they will exit the Configuration state — the protocol's
/// `RegistryDataPacket` is sender-driven and the client's
/// `KnownPacksManager` tracks per-registry completion. Sending only a
/// subset (e.g. just `dimension_type` + `worldgen/biome` as in stage 2)
/// causes the client to hang waiting for the missing registries.
///
/// The blobs themselves come from PrismarineJS minecraft-data, which
/// captured them from an actual 1.21.1 vanilla server.
pub fn all() -> &'static [RegistryEntry] {
    &[
        ("minecraft:dimension_type", dimension_type),
        ("minecraft:worldgen/biome", worldgen_biome),
        ("minecraft:chat_type", chat_type),
        ("minecraft:damage_type", damage_type),
        ("minecraft:trim_pattern", trim_pattern),
        ("minecraft:trim_material", trim_material),
        ("minecraft:wolf_variant", wolf_variant),
        ("minecraft:painting_variant", painting_variant),
        ("minecraft:banner_pattern", banner_pattern),
        ("minecraft:enchantment", enchantment),
        ("minecraft:jukebox_song", jukebox_song),
    ]
}
