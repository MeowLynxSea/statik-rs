pub mod common;
pub mod v1_20_1;
pub mod v1_21_1;

pub mod prelude {

    /// Cross-version shared types (Login Success property, abilities bit
    /// constants, Configuration KnownPack, the shared void-chunk builder).
    pub use crate::common::{abilities, void_chunk_bytes, KnownPack, Property};
}
