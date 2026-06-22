//! Runtime protocol-version dispatch.
//!
//! statik supports multiple Minecraft protocol versions compiled into a single
//! binary. The selected version (set via `--mc-version` / `[mc] version`) is
//! carried by [`ProtocolKind`] on each `Connection`; every version-sensitive
//! operation decodes / routes through a `match` on it.
//!
//! This is an `enum`-dispatch design (not `dyn`): the per-version packet
//! aggregations are distinct `#[derive(PacketGroup)]` types with version-
//! specific ids/fields, so they cannot share a single enum. Dispatch points
//! match on `ProtocolKind` and call into the selected version's modules.

use std::str::FromStr;

use statik_core::{handshake::ClientIntent, protocol::Protocol, state::State};
use statik_proto::{
    v1_20_1::{self, c2s::C2SPacket as C2SPacketV1_20_1},
    v1_21_1::{self, c2s::C2SPacket as C2SPacketV1_21_1},
};

impl FromStr for ProtocolKind {
    type Err = anyhow::Error;

    /// Parse a version selector string. Accepts the human-readable version
    /// (`"1.20.1"` / `"1.21.1"`) or the raw protocol number (`"763"` /
    /// `"767"`). Case-insensitive.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "1.20.1" | "763" => Self::V1_20_1(v1_20_1::V1_20_1),
            "1.21.1" | "767" => Self::V1_21_1(v1_21_1::V1_21_1),
            other => anyhow::bail!(
                "unsupported minecraft version \"{other}\". Supported: 1.20.1 (763), 1.21.1 (767)."
            ),
        })
    }
}

/// The selected Minecraft protocol version for a connection.
#[derive(Debug, Clone, Copy)]
pub enum ProtocolKind {
    V1_20_1(v1_20_1::V1_20_1),
    V1_21_1(v1_21_1::V1_21_1),
}

impl ProtocolKind {
    pub fn protocol_version(&self) -> usize {
        match self {
            Self::V1_20_1(p) => p.protocol_version(),
            Self::V1_21_1(p) => p.protocol_version(),
        }
    }

    pub fn minecraft_version(&self) -> &'static str {
        match self {
            Self::V1_20_1(p) => p.minecraft_version(),
            Self::V1_21_1(p) => p.minecraft_version(),
        }
    }
}

impl Default for ProtocolKind {
    fn default() -> Self {
        Self::V1_20_1(v1_20_1::V1_20_1)
    }
}

/// A C2S packet decoded against the connection's selected protocol version.
///
/// Each variant wraps that version's `C2SPacket` aggregation enum. Handlers
/// `match` on this to route per-version behaviour.
#[allow(clippy::large_enum_variant)]
pub enum DecodedC2S {
    V1_20_1(C2SPacketV1_20_1),
    V1_21_1(C2SPacketV1_21_1),
}

impl DecodedC2S {
    /// Decode a C2S packet body (the bytes after the length + optional
    /// compression framing) for the given protocol version and connection
    /// state.
    pub fn decode(
        protocol: ProtocolKind,
        state: State,
        buffer: &mut (impl std::io::Read + ?Sized),
    ) -> anyhow::Result<Self> {
        match protocol {
            ProtocolKind::V1_20_1(_) => {
                C2SPacketV1_20_1::decode_in_state(state, buffer).map(Self::V1_20_1)
            }
            ProtocolKind::V1_21_1(_) => {
                C2SPacketV1_21_1::decode_in_state(state, buffer).map(Self::V1_21_1)
            }
        }
    }
}

impl std::fmt::Debug for DecodedC2S {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V1_20_1(p) => f.debug_tuple("V1_20_1").field(p).finish(),
            Self::V1_21_1(p) => f.debug_tuple("V1_21_1").field(p).finish(),
        }
    }
}

/// Map a handshake [`ClientIntent`] onto a connection [`State`].
///
/// `Status` → [`State::Status`], `Login` → [`State::Login`]. `Transfer` is
/// unsupported (statik is not a transfer target) and returns `Err`.
pub fn intent_to_state(intent: ClientIntent) -> anyhow::Result<State> {
    Ok(match intent {
        ClientIntent::Status => State::Status,
        ClientIntent::Login => State::Login,
        ClientIntent::Transfer => anyhow::bail!(
            "Transfer handshake intention is not supported by statik (only Status / Login)."
        ),
    })
}
