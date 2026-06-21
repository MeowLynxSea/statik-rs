//! Minecraft [`BitSet`] wire format.
//!
//! A `BitSet` is encoded as a VarInt length (number of `i64` slots) followed
//! by that many `i64` values, all big-endian. Empty `BitSet` = `VarInt(0)`.
//!
//! Used by `S2CPlayerPosition` for its `relativeArguments` field and by the
//! chunk light update packet for its Y-mask bitsets.

use std::io::{Read, Write};

use crate::prelude::*;

/// A Minecraft wire-format `BitSet`.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct BitSet {
    /// Backing storage. Length of the encoded form equals `data.len()`.
    data: Vec<i64>,
}

impl BitSet {
    /// An empty `BitSet` (encodes as a single `0x00` byte).
    pub fn empty() -> Self {
        Self { data: Vec::new() }
    }

    /// Build a `BitSet` from raw `i64` slots.
    pub fn from_slots(slots: Vec<i64>) -> Self {
        Self { data: slots }
    }
}

impl Encode for BitSet {
    fn encode(&self, mut buffer: impl Write) -> Result<()> {
        VarInt(self.data.len() as i32).encode(&mut buffer)?;
        for v in &self.data {
            v.encode(&mut buffer)?;
        }
        Ok(())
    }
}

impl Decode for BitSet {
    fn decode(mut buffer: impl Read) -> Result<Self> {
        let len = VarInt::decode(&mut buffer)?.0 as usize;
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(i64::decode(&mut buffer)?);
        }
        Ok(Self { data })
    }
}
