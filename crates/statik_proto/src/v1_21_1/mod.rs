//! Minecraft 1.21.1 (protocol 767) packet definitions.
//!
//! All packets for 1.21.1 are collected under this version module. The
//! top-level [`C2SPacket`] / [`S2CPacket`] aggregations are decoded off the
//! wire by `Connection` when 1.21.1 is the selected protocol.

use statik_core::protocol::Protocol;

pub mod c2s;
pub mod registries;
pub mod s2c;

/// Marker type for the Minecraft 1.21.1 protocol.
#[derive(Debug, Clone, Copy, Default)]
pub struct V1_21_1;

impl Protocol for V1_21_1 {
    const PROTOCOL_VERSION: usize = 767;
    const MINECRAFT_VERSION: &'static str = "1.21.1";
}
