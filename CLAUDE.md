# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

`statik` is a lightweight, pure-Rust "fallback" Minecraft server. It does two things:

1. **Appear online** when queried — responds to status pings with the configured MOTD / icon / player count.
2. **Accept logins into a "limbo" world** — when a client connects and logs in, instead of disconnecting them, statik transitions them into the Play state in a void world at a fixed position with `isFlying=true`. This lets the actual Java server start on demand while clients are already connected and waiting.

statik supports **multiple Minecraft protocol versions at once** — both versions' packet definitions are compiled into the binary, and the version is selected per-process via the `--mc-version` CLI flag (or the `[mc] version` config field). The default is 1.20.1 / protocol 763. The currently supported versions are 1.20.1 and 1.21.1.

Limbo behavior is **unconditional**: every successful login transitions the client into Play state. There is no `enabled` flag — see [Limbo behavior](#limbo-behavior) below for details.

The MSRV is "the latest stable Rust." The `rustfmt.toml` uses unstable features and **must be run on nightly** (`cargo +nightly fmt`).

## Common commands

```bash
# Run (dev) — deps compiled with release, binary with debug assertions
cargo run

# Run (release)
cargo run --release

# Custom config path
cargo run -- --config=path/to/statik.toml
# Otherwise defaults to ./statik.toml, auto-generated if missing.

# CI checks (these run in .github/workflows on PRs and master pushes)
cargo test
cargo clippy
cargo +nightly fmt --all -- --check   # fmt check
cargo +nightly fmt --all              # apply formatting

# Run a single test
cargo test -p statik_core <test_name>
cargo test -p statik_proto <test_name>   # -p scopes to a workspace crate
```

When adding/changing packets or anything touching `statik_derive`, expect to recompile the proc-macro crate.

## Workspace architecture

Cargo workspace: the root `statik` binary + four crates under `crates/*`. Dependencies between them are declared via the workspace (`[workspace.dependencies]` in the root `Cargo.toml`), so use `statik_core = { workspace = true }` style imports in member `Cargo.toml`s.

- **`statik_core`** — protocol primitives shared by everything else. Defines `Encode`/`Decode`/`Packet` traits (`packet.rs`), `VarInt`, `State` enum (`Handshake`/`Status`/`Login`/`Configuration`/`Play` — note `Configuration = 3`, `Play = 4`), `ClientIntent` (handshake `next_state` / `intention`: `Status`/`Login`/`Transfer` — decoupled from `State` so `Transfer=3` doesn't collide with `Configuration=3`), `Chat`, `RawBytes`, `BitSet` (`bitset.rs`, Minecraft wire format `VarInt(len) + i64[]`), f32/f64 wire encoding (`impls.rs`), and the `prelude` (re-exports `anyhow`, `log` macros, core types). The `prelude::*` import is the canonical way to pull these in. Also defines the `Protocol` trait (`protocol.rs`) used by each per-version marker type to expose its protocol number / version name.
- **`statik_derive`** — proc-macro crate providing `#[derive(Packet)]`, `#[derive(PacketGroup)]`, `#[derive(Encode)]`, `#[derive(Decode)]`. This is the heart of the packet system; see below. **Note: `#[derive(Encode)]` / `#[derive(Decode)]` only support structs, not enums** — enums like `ClientIntent` need hand-written `Encode` / `Decode` impls.
- **`statik_proto`** — concrete Minecraft packets, **organised per protocol version** under `crates/statik_proto/src/v1_20_1/` and `crates/statik_proto/src/v1_21_1/`. Each version has its own `c2s/` (client→server) and `s2c/` (server→client) modules, then by `state` (handshake / status / login / [configuration] / play). Each packet is a plain struct annotated with `#[derive(Debug, Packet)]` and `#[packet(id = 0xNN, state = State::Xxx)]`. The top-level `C2SPacket` and `S2CPacket` enums (`v1_20_1/c2s.rs`, `v1_20_1/s2c.rs`, etc.) aggregate all packets per direction via `#[derive(PacketGroup)]` and are the types decoded off the wire. The cross-version shared types live in `statik_proto/src/common.rs` (`Property`, `KnownPack`, `abilities` bit constants, the `void_chunk_bytes` builder for the 24-section paletted-container empty-chunk payload used by both versions' `S2CLevelChunkWithLight`). Version-specific precomputed blobs (1.20.1's `registry_1_20_1.nbt` and 1.21.1's per-registry `.nbt` files) live under `v1_20_1/data/` and `v1_21_1/data/`.
- **`statik_server`** — the actual TCP server. `server::Server` binds two listeners (MC on `config.mc.port` default 25565, API on `config.api.port` default 8080) and spawns a `Handler` per MC connection. `connection::Connection` does framed packet I/O (VarInt-length-prefixed) and routes decoded `C2SPacket`s to `handle_handshake`/`handle_status`/`handle_login`/`handle_play` based on its current `State`. `Connection` also tracks `compression_threshold: Option<i32>`; when set, both inbound and outbound packets are zlib-compressed per the Minecraft compression framing (see [Compression](#compression)). `handler::Handler` wraps a connection + `Shutdown` and runs the per-client loop. `api::handle` serves a minimal newline-delimited JSON management API (`ping`/`status`/`shutdown`, auth via `config.api.token`) — the hook a supervisor uses to drive statik. `config::ServerConfig` is the `statik.toml` schema (serde, all sections `#[serde(default)]`), with `limbo` and `compression` sections controlling the in-world behavior and packet compression.
- **root `src/main.rs`** — CLI (`clap`), config loading/auto-generation, tracing init (`tracing-subscriber`'s `fmt().init()` already installs the log→tracing bridge via the `tracing-log` API — do **not** add an explicit `tracing_log::LogTracer::init()` after it, that will fail with "attempted to set a logger after the logging system was already initialized"), and the top-level `tokio::select!` shutdown loop (ctrl-c / SIGQUIT / SIGTERM / internal broadcast). Shutdown uses a `broadcast::Sender<String>` (disconnect reason) + `mpsc::Sender<String>` (`shutdown_complete_tx`) so the server waits for all connection tasks to drop before exiting. `src/quit.rs` wraps the OS signal futures.

## Packet system (how to add a packet)

1. Define the struct in the right version's `c2s/{state}.rs` or `s2c/{state}.rs` module — e.g. `crates/statik_proto/src/v1_20_1/c2s/play.rs` or `crates/statik_proto/src/v1_21_1/s2c/configuration.rs` — with `#[derive(Debug, Packet)]` and `#[packet(id = 0xNN, state = State::Xxx)]`. Field types must implement `Encode`/`Decode` (primitives via `statik_core::impls`, `VarInt`, `String`, `Chat`, etc.).
2. Add a variant to the corresponding version's `C2SPacket` / `S2CPacket` enum in `v1_XX/c2s.rs` / `v1_XX/s2c.rs` — `#[derive(PacketGroup)]` generates the `From<PacketType>` impls and a `pub fn decode_in_state(state, buffer)` inherent method that reads the leading VarInt packet id and dispatches to the variant whose `Packet::STATE` matches `state` and whose `Packet::ID` matches the id.
3. Wire handling in `statik_server/src/connection.rs` (`handle_handshake`/`handle_status`/`handle_login`/`handle_configuration`/`handle_play` — these take `DecodedC2S` and match on the per-version enum variant, then call into version-specific S2C packet constructors). For S2C packets, `Connection::write_packet` encodes the packet into a staging buffer prefixed by its VarInt length.

Notes / gotchas:
- `PacketGroup` generates `decode_in_state` (state-aware), **not** a `Decode` impl — packet ids are reused across states (e.g. `0x00` exists in Handshake, Status and Login for C2S), so decoding by id alone is ambiguous. Always decode via `C2SPacket::decode_in_state(self.state, buf)` for the version in use. The `Encode` impl for the group is still commented out; encode individual packet types via `write_packet`.
- `Packet` derive requires the `#[packet(...)]` attribute with at least `id = ...`. `state` defaults to `State::Play` if omitted — always specify it explicitly for non-Play packets (and use `State::Configuration` for 1.21.1 Configuration-state packets, not `State::Play`).
- `State` is serialized/deserialized as a raw discriminant (`Handshake`=0, `Status`=1, `Login`=2, `Configuration`=3, `Play`=4) and derives `PartialEq`/`Eq`/`Hash`. The handshake `next_state` field is **not** a `State` — it is the separate [`ClientIntent`](crates/statik_core/src/handshake.rs) enum (`Status`=1, `Login`=2, `Transfer`=3). `Transfer` is rejected at the handshake handler; `Configuration` is never reached via handshake — it is entered after `LoginSuccess` once the client sends `Login Acknowledged`.
- Config is shared as `Arc<RwLock<ServerConfig>>`; each MC connection task gets one clone passed into `Connection` (the `Handler` no longer holds its own copy).
- Round-trip tests live in `crates/statik_proto/tests/roundtrip.rs` and `crates/statik_core/src/varint.rs` — when adding/changing a packet, add a round-trip test there.
- When the handler's `handle_connection()` returns `Err(UnexpectedEof)` (peer closed TCP after we sent a disconnect), log at `debug!` and return `Ok(())`. `UnexpectedEof` is the **expected** end of every disconnect-driven flow, not a warning. Only escalate to `warn!` for non-EOF errors.

## Limbo behavior

statik unconditionally places every connecting player into the Play state in an empty void world at the configured fixed position. The `mc.disconnect_msg` field is no longer used on the login path (it's still useful as a default for `S2CDisconnectPlay` during shutdown). The login flow is **per-protocol-version** — see `Connection::handle_login` and `Connection::handle_configuration`.

### 1.20.1 login → play

`Connection::handle_login` (`C2SPacketV1_20_1::LoginStart` arm) emits packets in this exact order:

1. **Optional `S2CSetCompression`** — only sent if `[compression] enabled = true` in `statik.toml`. Must precede `LoginSuccess`; after this packet, `Connection.compression_threshold` is set and all subsequent packets in both directions are zlib-framed.
2. **`S2CLoginSuccess`** — always sent (offline-mode UUID: `Uuid::nil()` for now).
3. State transition: `Connection.state = State::Play`.
4. **Initial Play burst** (order matters):
   - `S2CSetChunkCacheCenter` (0x4E) — center at (0, 0).
   - `S2CSetChunkCacheRadius` (0x4F) — `limbo.view_distance`.
   - `S2CLevelChunkWithLight` (0x24) — empty air chunk at (0,0) so the client exits the "Loading Terrain" screen immediately. Body built once by `void_chunk_bytes()` (shared between versions via `statik_proto::common`).
   - `S2CSetDefaultSpawnPosition` (0x50) — sets the respawn anchor to `limbo.position`.
   - `S2CLogin` (0x28) — the Login Packet with the registry (built once by `registry_bytes()` in `v1_20_1::s2c::play`), the configured `dimension` / `game_type` / `view_distance`, and `is_flat = true`.
   - `S2CGameEvent` (0x1F) — `event = 7` (`START_WAITING_FOR_LEVELS`) to officially hand the player to the level-loading flow.
   - `S2CPlayerPosition` (0x3C) — teleport to `limbo.position` (absolute, `relative_arguments` empty, teleport id `0`).
   - `S2CPlayerAbilities` (0x34) — `invulnerable = true`, `is_flying = true`, `can_fly = true`. This prevents fall damage in the void.

### 1.21.1 login → configuration → play

1.21.1 splits login into a Login phase (just `Hello` + `LoginSuccess`) followed by a **Configuration** state (exchanged via `Login Acknowledged`) and only then the Play burst. Sequence in `Connection::handle_login` + `handle_configuration`:

1. **Optional `S2CSetCompression`** (same as 1.20.1).
2. **`S2CLoginSuccess`** — state **stays** at `State::Login`; we wait for the client's `Login Acknowledged`.
3. Client sends `C2SLoginAcknowledged` → state transitions to `State::Configuration` and `send_configuration_burst` emits the Configuration sequence:
   - `S2CCustomPayload` (0x01) — channel `minecraft:brand`, body `VarInt(6) || "statik"`.
   - `S2CFeatureFlags` (0x0C) — `["minecraft:vanilla"]`.
   - `S2CRegistryData` (0x07) × 11 — one per registry, body taken from `statik_proto::v1_21_1::registries::all()`. The blobs are precomputed from PrismarineJS minecraft-data (`tmp/minecraft-data/data/pc/1.21.1/loginPacket.json` → `build_tools/build_registry_nbt.py` → `crates/statik_proto/src/v1_21_1/data/registry_*.bin`). The 11 registries are `dimension_type`, `worldgen/biome`, `chat_type`, `damage_type`, `trim_pattern`, `trim_material`, `wolf_variant`, `painting_variant`, `banner_pattern`, `enchantment`, `jukebox_song` — the vanilla 1.21.1 client requires **all** of them before exiting Configuration.
   - `S2CKnownPacks` (0x0E) — `[{namespace: "minecraft", id: "core", version: "1.21.1"}]`.
4. Client processes that burst and replies with `C2SClientInformation` + `C2SSelectKnownPacks` + `C2SFinishConfiguration`. Other Configuration C2S packets (custom payload, keepalive, pong, resource pack response, cookie response) are silently logged and dropped.
5. On `C2SFinishConfiguration`:
   - `S2CFinishConfiguration` (0x03) — echo to the client.
   - State transition: `Connection.state = State::Play`.
   - `send_play_burst_v1_21_1` (the 1.21.1 Play burst):
     - `S2CLogin` (0x2B) — **fully-typed struct** (no more `RawBytes` placeholder). Field layout per PrismarineJS `play.toClient.packet_login`: `entity_id: i32`, `is_hardcore: bool`, `world_names: Vec<String>`, `max_players` / `view_distance` / `simulation_distance: VarInt`, `reduced_debug_info` / `enable_respawn_screen` / `do_limited_crafting: bool`, `world_state: SpawnInfo` (nested struct with `dimension`/`name`/`hashed_seed`/`gamemode i8`/`previous_gamemode u8`/`is_debug`/`is_flat`/`death: Option<DeathLocation>`/`portal_cooldown VarInt`), `enforces_secure_chat: bool`. Pulled from `cfg.limbo.*` / `cfg.mc.max_players`.
     - `S2CSetChunkCacheCenter` (0x54) — center at (0, 0).
     - `S2CSetChunkCacheRadius` (0x55) — `limbo.view_distance`.
     - `S2CLevelChunkWithLight` (0x27) — same wire format as 1.20.1, shares `void_chunk_bytes()`.
     - `S2CSetDefaultSpawnPosition` (0x56) — sets the respawn anchor to `limbo.position`.
     - `S2CPlayerPosition` (0x40) — teleport to `limbo.position`.
     - `S2CPlayerAbilities` (0x38) — same flags bitmask as 1.20.1.
     - `S2CGameEvent` (0x22) — `event = 7` (`START_WAITING_FOR_LEVELS`).

### Play state packet handling

`Connection::handle_play` matches on `DecodedC2S` and dispatches to the version-specific `C2SPacket` enum:

- `C2SAcceptTeleportation` (0x00) — log only; the client acked our teleport id.
- `C2SKeepAlive` (1.20.1: 0x12, 1.21.1: 0x18) — **response-driven keepalive**: reply with a fresh `S2CKeepAlive` carrying the same id. No timer task needed, and no contention with the read loop over `&mut self`. The Play keepalive timer in `handle_connection` (15 s) runs in parallel with the read loop and only fires in `State::Play`.
- `C2SPlayerPos` / `PosRot` / `Rot` (0x14/0x15/0x16) and `C2SPlayerStatusOnly` (1.20.1) / `C2SPlayerFlying` (1.21.1) (0x17) — ignored. The server doesn't track player position; flying mode keeps the client from falling.
- Any other Play packet — silently logged at `trace!` and dropped.

`[limbo]` config (`statik.toml`):
- `position = [x, y, z]` — fixed spawn / lock position. Default `[0.5, 64.0, 0.5]`.
- `gamemode = 1` — 0 survival, 1 creative, 2 adventure, 3 spectator. Default creative.
- `view_distance = 8` — chunk radius. Max 32 in vanilla.
- `simulation_distance = 8` — must be ≤ `view_distance`.
- `dimension = "minecraft:the_void"` — used for both `dimension` and `dimensionType` of the Login packet, and as the single entry of the `levels` set.

The `[limbo]` section has **no `enabled` flag** — limbo is the only behavior. The `[mc] disconnect_msg` field is preserved but unused (a future TODO might use it for graceful shutdown disconnect packets).

## Compression

When `[compression] enabled = true` in `statik.toml`, statik sends `S2CSetCompression` (Login 0x03) with `threshold` (default 256) right before `LoginSuccess`, then enables zlib framing on the connection. Minecraft compression framing:

- Outbound (`Connection::write_packet`): if the uncompressed body length ≥ `threshold`, write `VarInt(uncompressed_len) || zlib(body)`; otherwise write `VarInt(0) || body`. The outer `VarInt(length)` covers the entire framed payload including the Data Length prefix.
- Inbound (`Connection::try_parse_packet`): read the leading VarInt as `Data Length`. If 0, decode the rest as-is. If non-zero, zlib-decompress `Data Length` bytes and decode that.

Compression is implemented in `Connection::write_packet` / `Connection::try_parse_packet` using `flate2`. The flip is gated on `Connection.compression_threshold: Option<i32>` — `None` means no compression (default), `Some(n)` means enabled with threshold `n`. `SetCompression` itself is **always uncompressed** (compression flipping happens after it's sent).

## Protocol reference (`./tmp/`)

For any question about Minecraft packet shapes, ids, or field types, **prefer PrismarineJS minecraft-data** at `./tmp/minecraft-data/data/pc/<version>/`. The crucial files there:

- `protocol.json` — protodef schema with **real wire formats** (not Java field types), packet id ↔ name mapping, type definitions. Always check `<stage>.toClient.packet_<name>` / `<stage>.toServer.packet_<name>` for field order + types.
- `loginPacket.json` — concrete example of the `S2CLogin` packet body for that version, including `dimensionCodec` (the per-registry NBT entry trees used during Configuration). Drive `build_tools/build_registry_nbt.py` off this to regenerate `crates/statik_proto/src/v1_21_1/data/registry_*.bin`.
- `version.json` — protocol number ↔ MC version mapping.

The legacy `./tmp/mc-protocol-readmes/readme-<version>.md` files (wiki.vg dumps) are still useful for context but **`protocol.json` overrides them where they disagree** — the readmes' `Raw Type` column lists Java field types, not wire formats, and several fields differ.

### The readmes' `Raw Type` column lists Java field types, **not** wire formats

The packet tables have columns `Index | Type Index | Name | Raw Type | Full Type`. The `Raw Type` / `Full Type` columns reflect Mojang's Java field declarations in the source code, not necessarily the bytes on the wire. Several fields declare a Java type wider (or narrower) than how Mojang actually serialises them — most commonly:

| Readme says | Wire format actually is | Rust type to use | Why it matters |
|---|---|---|---|
| `int` (e.g. `protocolVersion`) | VarInt (1-5 bytes) | `VarInt` | Mojang uses `readVarInt()` despite the `int` field |
| `int` (e.g. `serverPort` in handshake) | `unsigned short` (2 bytes BE) | `u16` | Mojang calls `readUnsignedShort()` and stores it in an `int` field |
| `int` (e.g. `compressionThreshold` in Set Compression) | VarInt (1-5 bytes) | `VarInt` | Mojang calls `readVarInt()` |
| `long` (e.g. `time` in Ping/Pong, `id` in Keep Alive) | signed 8-byte BE | `i64` | Java `long` is signed |
| `int` (e.g. `sequence`, `transactionId`) | VarInt (1-5 bytes) | `VarInt` | Mojang calls `readVarInt()` |
| `String` (e.g. `hostName`, `name`, `username`) | VarInt length prefix + UTF-8 bytes | `String` | matches |
| `Optional<UUID>` (e.g. `profileId` in Login Start) | `bool` prefix + 16 bytes (or nothing) | `Option<Uuid>` | matches |
| `byte[]` (e.g. `publicKey`, `verifyToken`) | VarInt length prefix + bytes | `Vec<u8>` | matches |
| `GameProfile` (Login Success) | UUID + String + VarInt count + `Property[]` (each = String + String + Optional<String>) | `Uuid + String + Vec<Property>` | see `s2c/login.rs` for the `Property` struct |

**Rule of thumb:** when the readme says `int`/`long` for a field that isn't a position / count / block coordinate, treat it as VarInt (or `i64`) — but **always cross-check** by reading Mojang's source for that packet (the `FriendlyByteBuf` `read*` call). When in doubt, `i64` for `long`, `i32` is *rarely* the right answer in the play state.

The non-obvious cases above (notably the handshake `port` field) bit us once: a too-eager "the readme says int so use `i32`" change broke the handshake against the Notchian client. Treat any single-field `int` in a packet header / handshake / control packet as suspect and verify against Mojang before changing the type.

## CI

`.github/workflows/`: `continuous-integration` runs clippy (stable), `cargo +nightly fmt --all -- --check`, and `cargo test` on PRs and master pushes. The Docker workflow builds and publishes to `ghcr.io` on master pushes. PRs must pass `cargo test`, `cargo clippy`, and `cargo +nightly fmt`.
