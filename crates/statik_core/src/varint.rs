use std::io::{Read, Write};

use byteorder::{ReadBytesExt, WriteBytesExt};

use crate::prelude::*;

const SEGMENT_BITS: u8 = 0x7f;
const CONTINUE_BIT: u8 = 0x80;

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct VarInt(pub i32);

impl std::fmt::Display for VarInt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Encode for VarInt {
    fn encode(&self, mut buffer: impl Write) -> Result<()> {
        let mut value = self.0 as u32;

        loop {
            let part = value as u8;
            value >>= 7;
            if value == 0 {
                buffer.write_u8(part & 0x7f)?;
                break Ok(());
            } else {
                buffer.write_u8(part | 0x80)?;
            }
        }
    }
}

impl Decode for VarInt {
    fn decode(mut buffer: impl Read) -> Result<Self> {
        // Accumulate into a `u32` so that the final byte's `<< pos` cannot
        // trigger an arithmetic overflow (which panics in debug builds for
        // `i32`). The Minecraft VarInt is a signed 32-bit integer whose bits
        // are reinterpreted directly, so a single `as i32` cast at the end
        // recovers negative values correctly.
        let mut value: u32 = 0;
        let mut pos: u32 = 0;

        loop {
            let byte = buffer.read_u8()?;

            value |= ((byte & SEGMENT_BITS) as u32) << pos;

            if (byte & CONTINUE_BIT) == 0 {
                return Ok(VarInt(value as i32));
            }

            pos += 7;

            // A VarInt is at most 5 bytes (35 bits of position consumed).
            // If we still see a continuation bit after that, it's malformed.
            if pos >= 35 {
                return Err(anyhow!(
                    "Cannot decode VarInt! Exceeds maximum capacity of 5 bytes \
                     (2147483647/-2147483648)."
                ));
            }
        }
    }
}

impl From<i32> for VarInt {
    fn from(value: i32) -> Self {
        VarInt(value)
    }
}

impl From<u32> for VarInt {
    fn from(value: u32) -> Self {
        VarInt(value as i32)
    }
}

impl From<usize> for VarInt {
    fn from(value: usize) -> Self {
        VarInt(value as i32)
    }
}

impl From<isize> for VarInt {
    fn from(value: isize) -> Self {
        VarInt(value as i32)
    }
}

impl From<VarInt> for i32 {
    fn from(value: VarInt) -> Self {
        value.0
    }
}

impl From<VarInt> for u32 {
    fn from(value: VarInt) -> Self {
        value.0 as u32
    }
}

impl From<VarInt> for usize {
    fn from(value: VarInt) -> Self {
        value.0 as usize
    }
}

impl From<VarInt> for isize {
    fn from(value: VarInt) -> Self {
        value.0 as isize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(v: i32) {
        let mut buf = Vec::new();
        VarInt(v).encode(&mut buf).expect("encode");
        let decoded = VarInt::decode(&mut &buf[..]).expect("decode");
        assert_eq!(decoded.0, v, "roundtrip for {v} (bytes {buf:?})");
    }

    #[test]
    fn roundtrip_basic() {
        for &v in &[
            0,
            1,
            -1,
            127,
            128,
            255,
            1024,
            16_383,
            16_384,
            2_097_151,
            2_097_152,
            134_217_728,
            i32::MAX,
            i32::MIN,
            -2_097_151,
        ] {
            roundtrip(v);
        }
    }

    #[test]
    fn known_wire_bytes() {
        // Reference encodings from wiki.vg.
        let mut buf = Vec::new();
        VarInt(0).encode(&mut buf).unwrap();
        assert_eq!(buf, [0x00]);

        buf.clear();
        VarInt(127).encode(&mut buf).unwrap();
        assert_eq!(buf, [0x7f]);

        buf.clear();
        VarInt(128).encode(&mut buf).unwrap();
        assert_eq!(buf, [0x80, 0x01]);

        buf.clear();
        VarInt(255).encode(&mut buf).unwrap();
        assert_eq!(buf, [0xff, 0x01]);
    }

    #[test]
    fn rejects_overlong_varint() {
        // Six continuation bytes must be rejected, not panic.
        let malformed = [0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01];
        let result = VarInt::decode(&mut &malformed[..]);
        assert!(result.is_err(), "expected an error for an overlong VarInt");
    }

    #[test]
    fn negative_one_decodes_without_panic() {
        // Regression for the debug-build shift-overflow panic: -1 encodes to
        // `FF FF FF FF 0F` and must decode back to -1.
        let mut buf = Vec::new();
        VarInt(-1).encode(&mut buf).unwrap();
        assert_eq!(buf, [0xff, 0xff, 0xff, 0xff, 0x0f]);
        let decoded = VarInt::decode(&mut &buf[..]).unwrap();
        assert_eq!(decoded.0, -1);
    }
}
