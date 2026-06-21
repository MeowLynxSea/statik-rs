//! Minecraft's packed block position (`BlockPos`).
//!
//! On the wire a block position is a single signed 64-bit integer packing the
//! three coordinates: `((x & 0x3FFFFFF) << 38) | ((z & 0x3FFFFFF) << 12) |
//! (y & 0xFFF)`. `x` and `z` are 26-bit signed, `y` is 12-bit signed.

use crate::prelude::*;

/// A Minecraft block position, encoded as a packed `i64` on the wire.
///
/// Construct with [`BlockPos::new`]; the [`Encode`] / [`Decode`] impls handle
/// the bit-packing so packet structs can just hold a `BlockPos` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

const X_MASK: i64 = 0x3ff_ffff; // 26 bits
const Y_MASK: i64 = 0xfff; // 12 bits
const Z_MASK: i64 = 0x3ff_ffff; // 26 bits

impl BlockPos {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Pack into Minecraft's 64-bit wire encoding.
    pub fn to_packed(self) -> i64 {
        ((self.x as i64 & X_MASK) << 38)
            | ((self.z as i64 & Z_MASK) << 12)
            | (self.y as i64 & Y_MASK)
    }

    /// Unpack from Minecraft's 64-bit wire encoding, sign-extending each field.
    pub fn from_packed(value: i64) -> Self {
        // Shift left to put each field's sign bit at bit 63, then arithmetic
        // shift right to sign-extend back down.
        let x = (value >> 38) as i32;
        let y = ((value << 52) >> 52) as i32;
        let z = ((value << 26) >> 38) as i32;
        Self { x, y, z }
    }
}

impl Encode for BlockPos {
    fn encode(&self, buffer: impl std::io::Write) -> Result<()> {
        self.to_packed().encode(buffer)
    }
}

impl Decode for BlockPos {
    fn decode(buffer: impl std::io::Read) -> Result<Self> {
        Ok(Self::from_packed(i64::decode(buffer)?))
    }
}
