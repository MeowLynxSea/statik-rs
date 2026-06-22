//! Minecraft 1.20.1 (protocol 763) packet definitions.
//!
//! All packets for 1.20.1 are collected under this version module; the
//! top-level [`C2SPacket`] / [`S2CPacket`] aggregations are decoded off the
//! wire by `Connection` when 1.20.1 is the selected protocol.

use statik_core::protocol::Protocol;

pub mod c2s;
pub mod s2c;

/// Marker type for the Minecraft 1.20.1 protocol.
#[derive(Debug, Clone, Copy, Default)]
pub struct V1_20_1;

impl Protocol for V1_20_1 {
    const PROTOCOL_VERSION: usize = 763;
    const MINECRAFT_VERSION: &'static str = "1.20.1";
}
