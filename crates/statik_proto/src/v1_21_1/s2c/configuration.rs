//! Server-to-client packets in the Configuration phase (1.21.1, protocol 767).
//!
//! The limbo flow uses a small subset of these packets to drive the client
//! from `Login Acknowledged` through `Finish Configuration`:
//!
//! 1. `S2CCustomPayload` (0x01) — server brand ("minecraft:brand")
//! 2. `S2CFeatureFlags` (0x0C) — `["minecraft:vanilla", ...]`
//! 3. `S2CRegistryData` (0x07) × N — one per registry (placeholder payloads in
//!    stage 2; real data in stage 3)
//! 4. `S2CUpdateTags` (0x0D) — required by the vanilla client to leave
//!    Configuration; sent with an empty tag list (limbo has no tag-driven
//!    behaviour)
//! 5. `S2CKnownPacks` (0x0E) — vanilla datapack list
//! 6. (wait for `C2SFinishConfiguration`)
//! 7. `S2CFinishConfiguration` (0x03) — transition to Play

use statik_core::prelude::*;
use statik_derive::*;

use crate::common::{KnownPack, TagGroup};

/// 0x01 - Custom Payload (plugin message, e.g. `minecraft:brand`).
///
/// `data` is the remaining frame bytes ([`RawBytes`]).
#[derive(Debug, Packet)]
#[packet(id = 0x01, state = State::Configuration)]
pub struct S2CCustomPayload {
    pub channel: String,
    pub data: RawBytes,
}

/// 0x02 - Disconnect (Configuration).
#[derive(Debug, Packet)]
#[packet(id = 0x02, state = State::Configuration)]
pub struct S2CDisconnectConfiguration {
    pub reason: Chat,
}

/// 0x03 - Finish Configuration.
///
/// The server sends this in response to `C2SFinishConfiguration` to
/// transition the client to Play.
#[derive(Debug, Packet)]
#[packet(id = 0x03, state = State::Configuration)]
/// _no fields._
pub struct S2CFinishConfiguration {}

/// 0x04 - Keep Alive (Configuration).
///
/// `keep_alive_id` is `i64` (8-byte signed BE), same as the C2S variant —
/// confirmed against PrismarineJS `tmp/minecraft-data/data/pc/1.21.1/
/// protocol.json` (`configuration.toClient.packet_keep_alive`). statik
/// does not actively send Configuration keepalives in stage 2.
#[derive(Debug, Packet)]
#[packet(id = 0x04, state = State::Configuration)]
pub struct S2CConfigurationKeepAlive {
    pub keep_alive_id: i64,
}

/// 0x05 - Ping (Configuration).
#[derive(Debug, Packet)]
#[packet(id = 0x05, state = State::Configuration)]
pub struct S2CPingConfiguration {
    pub id: i32,
}

/// 0x07 - Registry Data.
///
/// One packet per registry. Field layout per PrismarineJS protocol.json
/// (`configuration.toClient.packet_registry_data`):
/// - `registry_id: String` — e.g. `"minecraft:dimension_type"`.
/// - `entries: Vec<RegistryEntry>` — VarInt-counted list of named entries.
///
/// Each entry's `value` is an optional anonymous-root NBT compound (the
/// stored NBT does *not* have its own outer name — just the type tag +
/// payload). statik keeps the per-entry NBT as a length-determined raw
/// byte sequence ([`RawBytes`] would be greedy here, so the `data` field
/// uses [`Vec<u8>`] inside the option to be bounded by the next entry).
///
/// **NOTE:** since [`anonymousNbt`](https://github.com/PrismarineJS/minecraft-data)
/// is a self-delimiting structure (TAG_Compound starts with `0x0a` and ends
/// with a matching `0x00`), and we only ever read or write these as
/// precomputed blobs, we model the value as an inline NBT byte run via
/// [`RawNbt`] which `Decode`s by walking the NBT tags and `Encode`s by
/// passing the bytes through. See [`crate::v1_21_1::data`] for the
/// precomputed blobs used by statik's Configuration burst.
#[derive(Debug, Packet)]
#[packet(id = 0x07, state = State::Configuration)]
pub struct S2CRegistryData {
    /// The registry id, e.g. `"minecraft:dimension_type"`.
    pub registry_id: String,
    /// The registry entries (each a `(String, Option<NBT>)` pair). Captured
    /// as the precomputed remainder of the frame via [`RawBytes`]; the
    /// caller is responsible for encoding the VarInt-count + entries.
    /// We keep this as a raw blob because the per-entry NBT requires
    /// walking the tag tree to know its length, and statik ships
    /// hand-built blobs from the `data/` folder for each registry.
    pub data: RawBytes,
}

/// 0x0C - Feature Flags.
///
/// A list of enabled feature flag ids. The vanilla client only requires
/// `minecraft:vanilla` to be present in this list.
#[derive(Debug, Packet)]
#[packet(id = 0x0C, state = State::Configuration)]
pub struct S2CFeatureFlags {
    pub features: Vec<String>,
}

/// 0x0D - Update Tags.
///
/// Lists every tag definition the server knows about, grouped by registry
/// (block, item, fluid, entity_type, ...). The vanilla client requires
/// this packet during Configuration — without it, the client never sends
/// `FinishConfiguration` and the connection stalls on "Joining World"
/// (the limbo Play burst never gets dispatched). The packet can be sent
/// with an empty `tags` list: the client only needs to receive it to
/// consider Configuration complete.
///
/// statik ships an empty tag list because the limbo world has no
/// tag-driven behaviour; replacing it with real tag data is a future
/// concern and would not require any structural change beyond the
/// `tags` field on this struct.
#[derive(Debug, Packet)]
#[packet(id = 0x0D, state = State::Configuration)]
pub struct S2CUpdateTags {
    pub tags: Vec<TagGroup>,
}

/// 0x0E - Known Packs.
///
/// The list of datapacks the server has available. The client picks a
/// subset via `C2SSelectKnownPacks`.
#[derive(Debug, Packet)]
#[packet(id = 0x0E, state = State::Configuration)]
pub struct S2CKnownPacks {
    pub packs: Vec<KnownPack>,
}
