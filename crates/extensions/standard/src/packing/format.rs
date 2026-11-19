//! `pack('n2C*', 1, 2, 3, 4)` reads a decimal count and a star count.

#[derive(Clone, Copy)]
pub(super) enum Endian {
    Native,
    Little,
    Big,
}

impl Endian {
    pub(super) fn little(self) -> bool {
        match self {
            Self::Native => cfg!(target_endian = "little"),
            Self::Little => true,
            Self::Big => false,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Number {
    Signed(usize),
    Unsigned(usize),
    Float,
    Double,
}

impl Number {
    pub(super) fn size(self) -> usize {
        match self {
            Self::Signed(size) | Self::Unsigned(size) => size,
            Self::Float => 4,
            Self::Double => 8,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Code {
    Numeric(Number, Endian),
    String(u8),
    Hex(u8),
    Skip,
    Back,
    Seek,
}

impl Code {
    pub(super) fn parse(byte: u8) -> Option<Self> {
        use Endian::{Big, Little, Native};
        use Number::{Double, Float, Signed, Unsigned};
        let (number, endian) = match byte {
            b'c' => (Signed(1), Native),
            b'C' => (Unsigned(1), Native),
            b's' => (Signed(2), Native),
            b'S' => (Unsigned(2), Native),
            b'n' => (Unsigned(2), Big),
            b'v' => (Unsigned(2), Little),
            b'i' | b'l' => (Signed(4), Native),
            b'I' | b'L' => (Unsigned(4), Native),
            b'N' => (Unsigned(4), Big),
            b'V' => (Unsigned(4), Little),
            b'q' => (Signed(8), Native),
            b'Q' => (Unsigned(8), Native),
            b'J' => (Unsigned(8), Big),
            b'P' => (Unsigned(8), Little),
            b'f' => (Float, Native),
            b'g' => (Float, Little),
            b'G' => (Float, Big),
            b'd' => (Double, Native),
            b'e' => (Double, Little),
            b'E' => (Double, Big),
            b'a' | b'A' | b'Z' => return Some(Self::String(byte)),
            b'h' | b'H' => return Some(Self::Hex(byte)),
            b'x' => return Some(Self::Skip),
            b'X' => return Some(Self::Back),
            b'@' => return Some(Self::Seek),
            _ => return None,
        };
        Some(Self::Numeric(number, endian))
    }
}

/// PHP parses runs of decimal repeat digits with the platform's signed long.
pub(super) fn count(format: &mut &[u8]) -> Option<i64> {
    if let Some(rest) = format.strip_prefix(b"*") {
        *format = rest;
        return None;
    }
    let mut digits = format.iter().copied().take_while(u8::is_ascii_digit);
    let Some(first) = digits.next() else {
        return Some(1);
    };
    let mut value = i64::from(first - b'0');
    let mut width = 1;
    for digit in digits {
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(digit - b'0'));
        width += 1;
    }
    *format = &format[width..];
    Some(value)
}

pub(super) fn diagnostic_byte(byte: u8) -> String {
    String::from_utf8_lossy(&[byte]).into_owned()
}
