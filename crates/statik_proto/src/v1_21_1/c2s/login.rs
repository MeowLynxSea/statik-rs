//! Client-to-server packets in the Login state for MC 1.21.1 (protocol 767).
//!
//! The Login state is the same as 1.20.1's on the wire for the first two
//! packets (`Hello`, `Key`) plus two new packets: `Custom Query Answer` (0x02)
//! and `Cookie Response` (0x04). `LoginStart` is renamed to `Hello` and
//! `profileId` becomes a **mandatory** `UUID` (no `Option`).
//!
//! 1.21+ also adds `Login Acknowledged` (0x03, no fields) which the client
//! sends after `LoginSuccess` to trigger the transition into Configuration.

use statik_core::prelude::*;
use statik_derive::*;
use uuid::Uuid;

/// 0x00 - Hello (was `LoginStart` in 1.20.1).
///
/// `profileId` is a **mandatory** `UUID` in 1.21+ (no `bool` prefix). statik
/// ignores the UUID in offline mode.
#[derive(Debug, Packet)]
#[packet(id = 0x00, state = State::Login)]
pub struct C2SHello {
    pub name: String,
    pub profile_id: Uuid,
}

/// 0x01 - Key (encryption response).
///
/// statik never sends an `EncryptionRequest`, so receiving this is a protocol
/// violation.
#[derive(Debug, Packet)]
#[packet(id = 0x01, state = State::Login)]
pub struct C2SKey {
    pub public_key: Vec<u8>,
    pub verify_token: Vec<u8>,
}

/// 0x02 - Custom Query Answer (response to a `LoginPluginRequest`).
#[derive(Debug, Packet)]
#[packet(id = 0x02, state = State::Login)]
pub struct C2SCustomQueryAnswer {
    pub transaction_id: VarInt,
    pub data: Option<RawBytes>,
}

/// 0x03 - Login Acknowledged.
///
/// The client sends this after `LoginSuccess` to acknowledge that it is ready
/// to enter the Configuration state. statik uses this as the trigger to begin
/// the Configuration handshake sequence.
#[derive(Debug, Packet)]
#[packet(id = 0x03, state = State::Login)]
/// _no fields._
pub struct C2SLoginAcknowledged {}

/// 0x04 - Cookie Response.
///
/// Renamed to `C2SLoginCookieResponse` to avoid a name collision with the
/// Configuration-state `C2SCookieResponse` (which lives in a different
/// state but shares the wire id 0x04 — different STATE means the
/// `PacketGroup` lookup can dispatch them correctly, but the type names
/// need to be distinct to glob-import both modules at once).
#[derive(Debug, Packet)]
#[packet(id = 0x04, state = State::Login)]
pub struct C2SLoginCookieResponse {
    pub key: String,
    pub payload: Option<RawBytes>,
}
