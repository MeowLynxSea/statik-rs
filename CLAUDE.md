# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

`statik` is a lightweight, pure-Rust "fallback" Minecraft server. Its purpose is to make a Minecraft server appear online (respond to status pings, accept logins) while the real Java server is offline, sending a disconnect message on login so the actual server can be started on demand. Target Minecraft version / protocol are pinned in `crates/statik_core/src/lib.rs` (`MINECRAFT_VERSION`, `PROTOCOL_VERSION`) — currently 1.20.1 / 763.

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

- **`statik_core`** — protocol primitives shared by everything else. Defines `Encode`/`Decode`/`Packet` traits (`packet.rs`), `VarInt`, `State` enum (`Handshake`/`Status`/`Login`/`Play`), `Chat`, `RawBytes`, and the `prelude` (re-exports `anyhow`, `log` macros, core types, version consts). The `prelude::*` import is the canonical way to pull these in.
- **`statik_derive`** — proc-macro crate providing `#[derive(Packet)]`, `#[derive(PacketGroup)]`, `#[derive(Encode)]`, `#[derive(Decode)]`. This is the heart of the packet system; see below.
- **`statik_proto`** — concrete Minecraft packets, split into `c2s/` (client→server) and `s2c/` (server→client), then by `state` (handshake / status / login). Each packet is a plain struct annotated with `#[derive(Packet)]` and `#[packet(id = 0xNN, state = State::Xxx)]`. The top-level `C2SPacket` and `S2CPacket` enums (`c2s.rs`, `s2c.rs`) aggregate all packets per direction via `#[derive(PacketGroup)]` and are the types decoded off the wire.
- **`statik_server`** — the actual TCP server. `server::Server` binds two listeners (MC on `config.mc.port` default 25565, API on `config.api.port` default 8080) and spawns a `Handler` per MC connection. `connection::Connection` does framed packet I/O (VarInt-length-prefixed) and routes decoded `C2SPacket`s to `handle_handshake`/`handle_status`/`handle_login` based on its current `State`. `handler::Handler` wraps a connection + `Shutdown` and runs the per-client loop. `api::handle` serves a minimal newline-delimited JSON management API (`ping`/`status`/`shutdown`, auth via `config.api.token`) — the hook a supervisor uses to drive statik. `config::ServerConfig` is the `statik.toml` schema (serde, all sections `#[serde(default)]`).
- **root `src/main.rs`** — CLI (`clap`), config loading/auto-generation, tracing init (`tracing-subscriber`'s `fmt().init()` already installs the log→tracing bridge via the `tracing-log` API — do **not** add an explicit `tracing_log::LogTracer::init()` after it, that will fail with "attempted to set a logger after the logging system was already initialized"), and the top-level `tokio::select!` shutdown loop (ctrl-c / SIGQUIT / SIGTERM / internal broadcast). Shutdown uses a `broadcast::Sender<String>` (disconnect reason) + `mpsc::Sender<String>` (`shutdown_complete_tx`) so the server waits for all connection tasks to drop before exiting. `src/quit.rs` wraps the OS signal futures.

## Packet system (how to add a packet)

1. Define the struct in the right `crates/statik_proto/src/{c2s,s2c}/{state}.rs` module, with `#[derive(Debug, Packet)]` and `#[packet(id = 0xNN, state = State::Xxx)]`. Field types must implement `Encode`/`Decode` (primitives via `statik_core::impls`, `VarInt`, `String`, `Chat`, etc.).
2. Add a variant to the corresponding `C2SPacket` / `S2CPacket` enum in `c2s.rs` / `s2c.rs` — `#[derive(PacketGroup)]` generates the `From<PacketType>` impls and a `pub fn decode_in_state(state, buffer)` inherent method that reads the leading VarInt packet id and dispatches to the variant whose `Packet::STATE` matches `state` and whose `Packet::ID` matches the id.
3. Wire handling in `statik_server/src/connection.rs` (`handle_handshake`/`handle_status`/`handle_login` match on the enum variant). For S2C packets, `Connection::write_packet` encodes the packet into a staging buffer prefixed by its VarInt length.

Notes / gotchas:
- `PacketGroup` generates `decode_in_state` (state-aware), **not** a `Decode` impl — packet ids are reused across states (e.g. `0x00` exists in Handshake, Status and Login for C2S), so decoding by id alone is ambiguous. Always decode via `C2SPacket::decode_in_state(self.state, buf)`. The `Encode` impl for the group is still commented out; encode individual packet types via `write_packet`.
- `Packet` derive requires the `#[packet(...)]` attribute with at least `id = ...`. `state` defaults to `State::Play` if omitted — always specify it explicitly for non-Play packets.
- `State` is serialized/deserialized as a raw discriminant (`Handshake`=0, `Status`=1, `Login`=2, `Play`=3) and derives `PartialEq`/`Eq`/`Hash` — this is how `next_state` in the handshake packet maps directly onto `Connection::state`.
- Config is shared as `Arc<RwLock<ServerConfig>>`; each MC connection task gets one clone passed into `Connection` (the `Handler` no longer holds its own copy).
- Round-trip tests live in `crates/statik_proto/tests/roundtrip.rs` and `crates/statik_core/src/varint.rs` — when adding/changing a packet, add a round-trip test there.
- When the handler's `handle_connection()` returns `Err(UnexpectedEof)` (peer closed TCP after we sent a disconnect), log at `debug!` and return `Ok(())`. `UnexpectedEof` is the **expected** end of every disconnect-driven flow, not a warning. Only escalate to `warn!` for non-EOF errors.

## Protocol reference (the readmes under `./tmp/`)

For any question about Minecraft packet shapes, ids, or field types, **always** start from `./tmp/mc-protocol-readmes/readme-<version>.md` (pick the version matching `PROTOCOL_VERSION` — currently `readme-1.20.1.md` for protocol 763). These are the bundled protocol dumps and are the canonical reference for this project.

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