//! Server-to-client packets in the Status state (1.21.1).

pub mod response;

use response::*;
use statik_core::prelude::*;
use statik_derive::*;

#[derive(Debug, Packet)]
#[packet(id = 0x00, state = State::Status)]
pub struct S2CStatusResponse {
    pub json_response: StatusResponse,
}

#[derive(Debug, Packet)]
#[packet(id = 0x01, state = State::Status)]
pub struct S2CPong {
    pub payload: i64,
}
