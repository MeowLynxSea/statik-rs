pub mod bitset;
pub mod chat;
pub mod handshake;
pub mod impls;
pub mod packet;
pub mod position;
pub mod protocol;
pub mod raw;
pub mod state;
pub mod varint;

pub mod prelude {

    pub use anyhow::{anyhow, bail, ensure, Context, Error, Result};
    pub use log::{debug, error, info, log, trace, warn};

    pub use crate::{
        bitset::*, chat::*, handshake::*, packet::*, position::*, protocol::*, raw::*, state::*,
        varint::*,
    };
}

/// Default Minecraft version string (1.20.1). Kept for backwards
/// compatibility; per-version metadata now lives in the `Protocol` impls under
/// `statik_proto`. Prefer `Protocol::MINECRAFT_VERSION` for new code.
pub const MINECRAFT_VERSION: &str = "1.20.1";
/// Default Minecraft protocol version (1.20.1 = 763). Kept for backwards
/// compatibility; prefer `Protocol::PROTOCOL_VERSION` for new code.
pub const PROTOCOL_VERSION: usize = 763;
