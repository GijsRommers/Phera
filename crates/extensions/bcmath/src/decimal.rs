//! Exact decimal coefficients, as in `bcadd('0.10', '0.2', 2) === '0.30'`.

use std::cmp::Ordering;

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{Signed, Zero};

#[derive(Clone)]
pub(crate) struct Decimal {
    pub(crate) coefficient: BigInt,
    pub(crate) scale: u32,
}

pub(crate) fn ten(power: u32) -> BigInt {
    BigInt::from(10).pow(power)
}

impl Decimal {
    pub(crate) fn integer(value: impl Into<BigInt>) -> Self {
        Self {
            coefficient: value.into(),
            scale: 0,
        }
    }

    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        // PHP's decimal parser stops at the first NUL byte.
        let bytes = bytes.split(|byte| *byte == 0).next()?;
        let (negative, digits) = match bytes {
            [b'-', rest @ ..] => (true, rest),
            [b'+', rest @ ..] => (false, rest),
            _ => (false, bytes),
        };
        let mut parts = digits.split(|byte| *byte == b'.');
        let whole = parts.next()?;
        let fraction = parts.next().unwrap_or_default();
        if parts.next().is_some() || !whole.iter().chain(fraction).all(u8::is_ascii_digit) {
            return None;
        }
        let scale = u32::try_from(fraction.len()).ok()?;
        let digits = [whole, fraction].concat();
        let coefficient = BigInt::parse_bytes(&digits, 10).unwrap_or_default();
        Some(Self {
            coefficient: if negative { -coefficient } else { coefficient },
            scale,
        })
    }

    pub(crate) fn at_scale(&self, scale: u32) -> BigInt {
        match scale.cmp(&self.scale) {
            _ if self.coefficient.is_zero() => BigInt::ZERO,
            Ordering::Greater => &self.coefficient * ten(scale - self.scale),
            Ordering::Less => {
                let places = self.scale - scale;
                if u64::from(places) >= self.coefficient.bits() {
                    BigInt::ZERO
                } else {
                    &self.coefficient / ten(places)
                }
            }
            Ordering::Equal => self.coefficient.clone(),
        }
    }

    pub(crate) fn resized(&self, scale: u32) -> Self {
        Self {
            coefficient: self.at_scale(scale),
            scale,
        }
    }

    pub(crate) fn render(&self) -> String {
        let digits = self.coefficient.magnitude().to_str_radix(10);
        let places = self.scale as usize;
        let mut result = String::new();
        if self.coefficient.is_negative() {
            result.push('-');
        }
        if places == 0 {
            result.push_str(&digits);
        } else if let Some(split) = digits.len().checked_sub(places).filter(|split| *split > 0) {
            result.push_str(&digits[..split]);
            result.push('.');
            result.push_str(&digits[split..]);
        } else {
            result.push_str("0.");
            result.extend(std::iter::repeat_n('0', places - digits.len()));
            result.push_str(&digits);
        }
        result
    }

    pub(crate) fn fraction(&self) -> bool {
        self.scale != 0 && self.coefficient != self.at_scale(0) * ten(self.scale)
    }

    pub(crate) fn compact_scale(&self) -> u32 {
        if self.coefficient.is_zero() {
            return 0;
        }
        let digits = self.coefficient.magnitude().to_str_radix(10);
        let zeros = digits
            .bytes()
            .rev()
            .take_while(|byte| *byte == b'0')
            .count();
        self.scale.saturating_sub(zeros as u32)
    }

    pub(crate) fn add(&self, other: &Self, subtract: bool) -> Self {
        let scale = self.scale.max(other.scale);
        let left = self.at_scale(scale);
        let right = other.at_scale(scale);
        Self {
            coefficient: if subtract { left - right } else { left + right },
            scale,
        }
    }

    pub(crate) fn multiply(&self, other: &Self) -> Self {
        Self {
            coefficient: &self.coefficient * &other.coefficient,
            scale: self.scale + other.scale,
        }
    }

    pub(crate) fn divide(&self, other: &Self, scale: u32) -> Self {
        let target = i64::from(other.scale) + i64::from(scale) - i64::from(self.scale);
        let coefficient = if self.coefficient.is_zero() {
            BigInt::ZERO
        } else if target >= 0 {
            &self.coefficient * ten(target as u32) / &other.coefficient
        } else {
            &self.coefficient / (&other.coefficient * ten((-target) as u32))
        };
        Self { coefficient, scale }
    }

    pub(crate) fn remainder(&self, other: &Self, scale: u32) -> Self {
        let common = self.scale.max(other.scale);
        Self {
            coefficient: self.at_scale(common) % other.at_scale(common),
            scale: common,
        }
        .resized(scale)
    }

    pub(crate) fn sqrt(&self, scale: u32) -> Self {
        Self {
            coefficient: self.at_scale(scale * 2).sqrt(),
            scale,
        }
    }

    pub(crate) fn compare(&self, other: &Self, scale: u32) -> Ordering {
        let scale = scale.min(self.scale.max(other.scale));
        self.at_scale(scale).cmp(&other.at_scale(scale))
    }

    pub(crate) fn round(&self, precision: i64, mode: Rounding) -> Self {
        if precision >= i64::from(self.scale) {
            return self.resized(precision as u32);
        }
        let places = i64::from(self.scale).saturating_sub(precision) as u64;
        let (mut quotient, half, has_remainder) =
            if places >= self.coefficient.bits().saturating_add(1) {
                (BigInt::ZERO, Ordering::Less, !self.coefficient.is_zero())
            } else {
                let unit = ten(places as u32);
                let (quotient, remainder) = self.coefficient.div_rem(&unit);
                let half = (remainder.abs() * 2u8).cmp(&unit);
                (quotient, half, !remainder.is_zero())
            };
        let negative = self.coefficient.is_negative();
        let increase = has_remainder
            && match mode {
                Rounding::Toward => false,
                Rounding::Away => true,
                Rounding::Floor => negative,
                Rounding::Ceiling => !negative,
                _ => {
                    half == Ordering::Greater
                        || (half == Ordering::Equal
                            && match mode {
                                Rounding::HalfAway => true,
                                Rounding::HalfToward => false,
                                Rounding::HalfEven => quotient.is_odd(),
                                Rounding::HalfOdd => quotient.is_even(),
                                _ => false,
                            })
                }
            };
        if increase {
            quotient += if negative { -1 } else { 1 };
        }
        if precision >= 0 {
            Self {
                coefficient: quotient,
                scale: precision as u32,
            }
        } else if quotient.is_zero() {
            Self::integer(0)
        } else {
            Self::integer(quotient * ten(precision.unsigned_abs() as u32))
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Rounding {
    HalfAway,
    HalfToward,
    HalfEven,
    HalfOdd,
    Away,
    Toward,
    Ceiling,
    Floor,
}
