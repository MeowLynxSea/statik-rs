use std::io::{Read, Write};

use crate::{
    packet::{Decode, Encode},
    prelude::{bail, Result},
    varint::VarInt,
};

/// The `next_state` / `intention` field of the handshake packet, decoupled from
/// [`crate::state::State`].
///
/// On the wire this is a `VarInt`: `1` = Status, `2` = Login, `3` = Transfer.
/// statik does not support Transfer; a `Transfer` intention is rejected at the
/// handshake handler. Configuration is **not** a handshake intention — it is
/// entered post-`LoginSuccess` via `Login Acknowledged`.
///
/// This is kept separate from [`crate::state::State`] so that adding
/// `State::Configuration = 3` does not collide with the Transfer handshake
/// value, and so future handshake intentions can evolve independently of the
/// connection state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientIntent {
    Status = 1,
    Login = 2,
    Transfer = 3,
}

impl Encode for ClientIntent {
    fn encode(&self, buffer: impl Write) -> Result<()> {
        VarInt(*self as i32).encode(buffer)
    }
}

impl Decode for ClientIntent {
    fn decode(buffer: impl Read) -> Result<Self> {
        Ok(match VarInt::decode(buffer)?.0 {
            1 => Self::Status,
            2 => Self::Login,
            3 => Self::Transfer,
            n => bail!(
                "parsed VarInt returned an invalid ClientIntent: {n}. Only values 1, 2 and 3 are \
                 valid."
            ),
        })
    }
}
