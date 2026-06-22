pub mod common;
pub mod v1_20_1;
pub mod v1_21_1;

pub mod prelude {

    /// Cross-version shared types: Login Success property, abilities bit
    /// constants, Configuration KnownPack, Tag / TagGroup. Chunk payloads
    /// are **not** shared across versions — the 1.20.1 and 1.21.1
    /// `S2CLevelChunkWithLight` heightmaps NBT formats differ
    /// (named vs. anonymous NBT), so use the per-version re-exports
    /// `v1_20_1::s2c::play::void_chunk_bytes_v1_20_1` /
    /// `v1_21_1::s2c::play::void_chunk_bytes_v1_21_1` instead.
    pub use crate::common::{
        abilities, void_chunk_bytes_v1_20_1, void_chunk_bytes_v1_21_1, KnownPack, Property,
    };
}
