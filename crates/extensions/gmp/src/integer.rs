//! PHP base detection and conversion, e.g. `gmp_init('0o17')` is fifteen.

use num_bigint::{BigInt, Sign};
use num_traits::ToPrimitive;
use phera_extension_api::{NativeError, Value, args};

use crate::object;

pub(crate) type Result<T> = std::result::Result<T, NativeError>;

#[derive(Clone, Copy)]
pub(crate) struct Argument(pub &'static str, pub usize, pub &'static str);

impl Argument {
    pub(crate) fn value_error(self, detail: &str) -> NativeError {
        let Self(function, index, parameter) = self;
        NativeError::value_error(format!(
            "{function}(): Argument #{index} (${parameter}) {detail}"
        ))
    }

    pub(crate) fn read(self, values: &[Value]) -> Result<BigInt> {
        match args::value(values, self.1 - 1) {
            Value::String(string) => parse(string.as_bytes(), 0)
                .ok_or_else(|| self.value_error("is not an integer string")),
            Value::Object(object) => Ok(object::value(object)),
            Value::Int(integer) => Ok(BigInt::from(*integer)),
            _ => Err(self.value_error("is not an integer string")),
        }
    }

    pub(crate) fn nonnegative(self, number: i64) -> Result<u64> {
        u64::try_from(number).map_err(|_| self.value_error("must be greater than or equal to 0"))
    }

    pub(crate) fn init(self, raw: &Value, base: i64) -> Result<BigInt> {
        if base != 0 && !(2..=62).contains(&base) {
            return Err(Argument(self.0, 2, "base").value_error("must be 0 or between 2 and 62"));
        }
        match raw {
            Value::String(string) => parse(string.as_bytes(), base as u32)
                .ok_or_else(|| self.value_error("is not an integer string")),
            Value::Int(integer) => Ok(BigInt::from(*integer)),
            _ => Ok(BigInt::ZERO),
        }
    }
}

pub(crate) fn unary(function: &'static str, values: &[Value]) -> Result<BigInt> {
    Argument(function, 1, "num").read(values)
}

pub(crate) fn pair(function: &'static str, values: &[Value]) -> Result<(BigInt, BigInt)> {
    Ok((
        Argument(function, 1, "num1").read(values)?,
        Argument(function, 2, "num2").read(values)?,
    ))
}

pub(crate) fn check(bytes: &[u8]) -> Option<&'static str> {
    parse(bytes, 0)
        .is_none()
        .then_some("is not an integer string")
}

pub(crate) fn parse(bytes: &[u8], base: u32) -> Option<BigInt> {
    let bytes = bytes.split(|byte| *byte == 0).next()?.trim_ascii_start();
    let prefix = prefix(bytes).filter(|radix| base == 0 || base == *radix);
    let (base, bytes) = match prefix {
        Some(base) => (base, bytes[2..].trim_ascii_start()),
        None => (base, bytes),
    };
    let (sign, bytes) = if let Some(bytes) = bytes.strip_prefix(b"-") {
        if bytes.first()?.is_ascii_whitespace() {
            return None;
        }
        (Sign::Minus, bytes)
    } else {
        (Sign::Plus, bytes)
    };
    let (base, bytes) = if base == 0 {
        match bytes {
            [b'0', b'x' | b'X', rest @ ..] => (16, rest),
            [b'0', b'b' | b'B', rest @ ..] => (2, rest),
            [b'0', ..] => (8, bytes),
            _ => (10, bytes),
        }
    } else {
        (base, bytes)
    };
    let digits = bytes
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .map(|byte| match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'A'..=b'Z' => Some(byte - b'A' + 10),
            b'a'..=b'z' => Some(byte - b'a' + if base > 36 { 36 } else { 10 }),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    if digits.is_empty() {
        return None;
    }
    BigInt::from_radix_be(sign, &digits, base)
}

fn prefix(bytes: &[u8]) -> Option<u32> {
    match bytes {
        [b'0', b'x' | b'X', ..] => Some(16),
        [b'0', b'b' | b'B', ..] => Some(2),
        [b'0', b'o' | b'O', ..] => Some(8),
        _ => None,
    }
}

pub(crate) fn render(number: &BigInt, base: i64) -> String {
    if (2..=36).contains(&base) {
        return number.to_str_radix(base as u32);
    }
    if base < 0 {
        return number.to_str_radix((-base) as u32).to_ascii_uppercase();
    }
    let (sign, digits) = number.to_radix_be(base as u32);
    let prefix = (sign == Sign::Minus).then_some('-');
    prefix
        .into_iter()
        .chain(digits.into_iter().map(|digit| {
            char::from(match digit {
                0..=9 => b'0' + digit,
                10..=35 => b'A' + digit - 10,
                _ => b'a' + digit - 36,
            })
        }))
        .collect()
}

pub(crate) fn to_int(number: &BigInt) -> i64 {
    if let Some(integer) = number.to_i64() {
        return integer;
    }
    let low = number.iter_u64_digits().next().unwrap_or(0);
    let magnitude = (low & i64::MAX as u64) as i64;
    if number.sign() == Sign::Minus && magnitude == 0 {
        i64::MIN
    } else if number.sign() == Sign::Minus {
        -magnitude
    } else {
        magnitude
    }
}

pub(crate) fn to_float(number: &BigInt) -> f64 {
    // GMP truncates the discarded bits in mpz_get_d toward zero.
    let shift = number.bits().saturating_sub(53);
    let significant = (number.magnitude() >> shift).to_u64().unwrap_or_default();
    let absolute = if shift >= 1024 {
        f64::INFINITY
    } else {
        significant as f64 * 2.0f64.powi(shift as i32)
    };
    if number.sign() == Sign::Minus {
        -absolute
    } else {
        absolute
    }
}
