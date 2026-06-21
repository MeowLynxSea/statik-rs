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
- **`statik_server`** — the actual TCP server. `server::Server` binds two listeners (MC on `config.mc.port` default 25565, API on `config.api.port` default 8080) and spawns a `Handler` per MC connection. `connection::Connection` does framed packet I/O (VarInt-length-prefixed) and routes decoded `C2SPacket`s to `handle_handshake`/`handle_status`/`handle_login` based on its current `State`. `handler::Handler` wraps a connection + `Shutdown` and runs the per-client loop. `config::ServerConfig` is the `statik.toml` schema (serde, all sections `#[serde(default)]`).
- **root `src/main.rs`** — CLI (`clap`), config loading/auto-generation, tracing init, and the top-level `tokio::select!` shutdown loop (ctrl-c / SIGQUIT / SIGTERM / internal broadcast). Shutdown uses a `broadcast::Sender<String>` (disconnect reason) + `mpsc::Sender<String>` (`shutdown_complete_tx`) so the server waits for all connection tasks to drop before exiting. `src/quit.rs` wraps the OS signal futures.

## Packet system (how to add a packet)

1. Define the struct in the right `crates/statik_proto/src/{c2s,s2c}/{state}/` module, with `#[derive(Debug, Packet)]` and `#[packet(id = 0xNN, state = State::Xxx)]`. Field types must implement `Encode`/`Decode` (primitives via `statik_core::impls`, `VarInt`, `String`, `Chat`, etc.).
2. Add a variant to the corresponding `C2SPacket` / `S2CPacket` enum in `c2s.rs` / `s2c.rs` — `#[derive(PacketGroup)]` generates the `From<PacketType>` impls and the `Decode` that matches on the leading VarInt packet ID and dispatches to the right variant's `decode`.
3. Wire handling in `statik_server/src/connection.rs` (`handle_handshake`/`handle_status`/`handle_login` match on the enum variant). For S2C packets, `Connection::write_packet` encodes the packet into a staging buffer prefixed by its VarInt length.

Notes / gotchas:
- `PacketGroup` currently only generates `Decode` (the `Encode` impl is commented out in `statik_derive/src/packet_group.rs`). Encode goes through the individual packet's `Encode` via `write_packet`.
- `Packet` derive requires the `#[packet(...)]` attribute with at least `id = ...`. `state` defaults to `State::Play` if omitted.
- `State` is serialized/deserialized as a raw discriminant (`Handshake`=0, `Status`=1, `Login`=2, `Play`=3) — this is how `next_state` in the handshake packet maps directly onto `Connection::state`.
- Config is shared as `Arc<RwLock<ServerConfig>>`; the API listener in `Server::run` is currently a `todo!()`.

## CI

`.github/workflows/`: `continuous-integration` runs clippy (stable), `cargo +nightly fmt --all -- --check`, and `cargo test` on PRs and master pushes. The Docker workflow builds and publishes to `ghcr.io` on master pushes. PRs must pass `cargo test`, `cargo clippy`, and `cargo +nightly fmt`.