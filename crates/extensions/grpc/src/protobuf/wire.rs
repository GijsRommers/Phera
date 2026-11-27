//! The protobuf wire format: varints, fixed-width numbers and
//! length-delimited byte runs, each behind a tag of field number and wire type.
//!
//! ```text
//! field 1 = 150 (int32)   ->  08 96 01
//!                             |  '---'- varint 150, seven bits per byte
//!                             '------- tag: (1 << 3) | wire type 0
//! ```

use std::fmt;

/// How a value is laid out after its tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireType {
    Varint = 0,
    Fixed64 = 1,
    Len = 2,
    StartGroup = 3,
    EndGroup = 4,
    Fixed32 = 5,
}

impl WireType {
    fn from_bits(bits: u8) -> Option<Self> {
        Some(match bits {
            0 => Self::Varint,
            1 => Self::Fixed64,
            2 => Self::Len,
            3 => Self::StartGroup,
            4 => Self::EndGroup,
            5 => Self::Fixed32,
            _ => return None,
        })
    }
}

/// Why bytes are not a valid protobuf message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    Truncated,
    VarintTooLong,
    InvalidWireType(u8),
    InvalidFieldNumber,
    /// Groups are a proto2 feature that no current schema should use.
    Group,
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("unexpected end of message"),
            Self::VarintTooLong => f.write_str("varint is longer than 10 bytes"),
            Self::InvalidWireType(bits) => write!(f, "invalid wire type {bits}"),
            Self::InvalidFieldNumber => f.write_str("invalid field number 0"),
            Self::Group => f.write_str("groups are not supported"),
        }
    }
}

/// The largest field number protobuf allows.
pub const MAX_FIELD_NUMBER: u32 = (1 << 29) - 1;

#[inline]
pub fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// A varint in a stack buffer, for patching a length into place.
#[inline]
pub fn varint_bytes(mut value: u64) -> ([u8; 10], usize) {
    let mut bytes = [0u8; 10];
    let mut len = 0;
    while value >= 0x80 {
        bytes[len] = (value as u8) | 0x80;
        value >>= 7;
        len += 1;
    }
    bytes[len] = value as u8;
    (bytes, len + 1)
}

#[inline]
pub fn put_tag(out: &mut Vec<u8>, number: u32, wire: WireType) {
    put_varint(out, (u64::from(number) << 3) | wire as u64);
}

/// How many bytes `put_varint` writes for `value`.
#[inline]
pub fn varint_len(value: u64) -> usize {
    // One byte per started group of seven bits; zero still takes a byte.
    let bits = 64 - (value | 1).leading_zeros() as usize;
    bits.div_ceil(7)
}

#[inline]
pub fn zigzag32(value: i32) -> u32 {
    ((value << 1) ^ (value >> 31)) as u32
}

#[inline]
pub fn zigzag64(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

#[inline]
pub fn unzigzag32(value: u32) -> i32 {
    ((value >> 1) as i32) ^ -((value & 1) as i32)
}

#[inline]
pub fn unzigzag64(value: u64) -> i64 {
    ((value >> 1) as i64) ^ -((value & 1) as i64)
}

/// A cursor over one message's bytes. Length-delimited values are returned
/// as slices of the input: decoding copies bytes only into PHP strings.
pub struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.position >= self.bytes.len()
    }

    #[inline]
    pub fn varint(&mut self) -> Result<u64, WireError> {
        // Most varints on the wire (tags, small numbers, lengths) are one byte.
        match self.bytes.get(self.position) {
            Some(&byte) if byte < 0x80 => {
                self.position += 1;
                Ok(u64::from(byte))
            }
            Some(_) => self.varint_slow(),
            None => Err(WireError::Truncated),
        }
    }

    #[cold]
    fn varint_slow(&mut self) -> Result<u64, WireError> {
        let mut value = 0u64;
        for shift in (0..70).step_by(7) {
            let byte = *self.bytes.get(self.position).ok_or(WireError::Truncated)?;
            self.position += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte < 0x80 {
                return Ok(value);
            }
        }
        Err(WireError::VarintTooLong)
    }

    #[inline]
    pub fn fixed32(&mut self) -> Result<u32, WireError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    #[inline]
    pub fn fixed64(&mut self) -> Result<u64, WireError> {
        let bytes = self.take(8)?;
        let mut array = [0u8; 8];
        array.copy_from_slice(bytes);
        Ok(u64::from_le_bytes(array))
    }

    /// A length-delimited value: its length as a varint, then that many bytes.
    #[inline]
    pub fn len_delimited(&mut self) -> Result<&'a [u8], WireError> {
        let len = self.varint()?;
        let len = usize::try_from(len).map_err(|_| WireError::Truncated)?;
        self.take(len)
    }

    /// The next field's number and wire type.
    #[inline]
    pub fn tag(&mut self) -> Result<(u32, WireType), WireError> {
        let key = self.varint()?;
        let wire = WireType::from_bits((key & 7) as u8)
            .ok_or(WireError::InvalidWireType((key & 7) as u8))?;
        let number = key >> 3;
        if number == 0 || number > u64::from(MAX_FIELD_NUMBER) {
            return Err(WireError::InvalidFieldNumber);
        }
        Ok((number as u32, wire))
    }

    /// Skips a value of a field the schema does not know.
    pub fn skip(&mut self, wire: WireType) -> Result<(), WireError> {
        match wire {
            WireType::Varint => self.varint().map(drop),
            WireType::Fixed64 => self.take(8).map(drop),
            WireType::Len => self.len_delimited().map(drop),
            WireType::Fixed32 => self.take(4).map(drop),
            WireType::StartGroup | WireType::EndGroup => Err(WireError::Group),
        }
    }

    #[inline]
    fn take(&mut self, len: usize) -> Result<&'a [u8], WireError> {
        let end = self
            .position
            .checked_add(len)
            .filter(|&end| end <= self.bytes.len())
            .ok_or(WireError::Truncated)?;
        let slice = &self.bytes[self.position..end];
        self.position = end;
        Ok(slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_round_trip_at_every_length() {
        for value in [
            0,
            1,
            127,
            128,
            150,
            16_383,
            16_384,
            u64::from(u32::MAX),
            u64::MAX,
        ] {
            let mut out = Vec::new();
            put_varint(&mut out, value);
            assert_eq!(out.len(), varint_len(value), "{value}");
            assert_eq!(Reader::new(&out).varint(), Ok(value));
        }
        let mut out = Vec::new();
        put_varint(&mut out, 150);
        assert_eq!(out, [0x96, 0x01]);
    }

    #[test]
    fn zigzag_maps_small_magnitudes_to_small_numbers() {
        assert_eq!(zigzag32(0), 0);
        assert_eq!(zigzag32(-1), 1);
        assert_eq!(zigzag32(1), 2);
        assert_eq!(zigzag32(i32::MIN), u32::MAX);
        assert_eq!(zigzag64(-2), 3);
        for value in [0, -1, 1, i32::MAX, i32::MIN] {
            assert_eq!(unzigzag32(zigzag32(value)), value);
        }
        for value in [0, -1, 1, i64::MAX, i64::MIN] {
            assert_eq!(unzigzag64(zigzag64(value)), value);
        }
    }

    #[test]
    fn malformed_input_is_reported() {
        assert_eq!(Reader::new(&[0x80]).varint(), Err(WireError::Truncated));
        assert_eq!(
            Reader::new(&[0xff; 11]).varint(),
            Err(WireError::VarintTooLong)
        );
        assert_eq!(Reader::new(&[0x0a, 5, 1]).tag().map(|t| t.0), Ok(1));
        let mut reader = Reader::new(&[0x0a, 5, 1]);
        reader.tag().unwrap();
        assert_eq!(reader.len_delimited(), Err(WireError::Truncated));
        assert_eq!(
            Reader::new(&[0x00]).tag(),
            Err(WireError::InvalidFieldNumber)
        );
        assert_eq!(
            Reader::new(&[0x0e]).tag(),
            Err(WireError::InvalidWireType(6))
        );
    }
}
