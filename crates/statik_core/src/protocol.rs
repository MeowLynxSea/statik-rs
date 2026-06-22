/// A complete Minecraft protocol version implementation.
///
/// Each supported MC version ships a zero-sized marker type implementing this
/// trait (e.g. `statik_proto::v1_20_1::V1_20_1`). The `statik_server` layer
/// dispatches per-version behaviour through
/// [`ProtocolKind`](../../statik_server/protocol/enum.ProtocolKind.html)
/// by matching on the selected version rather than via `dyn`, so this trait
/// carries only static version metadata.
pub trait Protocol: Send + Sync + 'static {
    /// The Minecraft protocol number (e.g. `763` for 1.20.1, `767` for 1.21.1).
    const PROTOCOL_VERSION: usize;
    /// Human-readable Minecraft version string (e.g. `"1.20.1"`).
    const MINECRAFT_VERSION: &'static str;

    /// The Minecraft protocol number.
    fn protocol_version(&self) -> usize {
        Self::PROTOCOL_VERSION
    }

    /// Human-readable Minecraft version string.
    fn minecraft_version(&self) -> &'static str {
        Self::MINECRAFT_VERSION
    }
}
