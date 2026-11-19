//! BCMath operations shared by functions, number methods, and operators.
//! `(new BcMath\Number('2.00'))->div(4)->value` is `'0.50'`.

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};
use phera_extension_api::{ArrayKey, NativeError, Value};

use crate::decimal::{Decimal, Rounding};

pub(crate) type Result<T> = std::result::Result<T, NativeError>;

#[derive(Clone, Copy)]
pub(crate) enum Operation {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

#[derive(Clone, Copy)]
pub(crate) struct Argument(pub &'static str, pub usize, pub &'static str);

impl Argument {
    pub(crate) fn invalid(self, detail: &str) -> NativeError {
        let Self(function, position, name) = self;
        NativeError::value_error(format!(
            "{function}(): Argument #{position} (${name}) {detail}"
        ))
    }

    pub(crate) fn parse(self, bytes: &[u8]) -> Result<Decimal> {
        Decimal::parse(bytes).ok_or_else(|| self.invalid("is not well-formed"))
    }

    pub(crate) fn scale(self, scale: i64) -> Result<u32> {
        if (0..=i64::from(i32::MAX)).contains(&scale) {
            Ok(scale as u32)
        } else {
            Err(self.invalid("must be between 0 and 2147483647"))
        }
    }

    pub(crate) fn precision(self, precision: i64) -> Result<i64> {
        if precision > i64::from(i32::MAX) {
            Err(self.invalid("must be between -9223372036854775808 and 2147483647"))
        } else {
            Ok(precision)
        }
    }
}

pub(crate) fn nonzero(number: &Decimal, operation: &str) -> Result<()> {
    if number.coefficient.is_zero() {
        Err(NativeError::throw(
            "DivisionByZeroError",
            format!("{operation} by zero"),
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn rounding(mode: Option<&Value>, argument: Argument) -> Result<Rounding> {
    let Some(mode) = mode else {
        return Ok(Rounding::HalfAway);
    };
    let name = if let Value::Object(object) = mode {
        object.get(&ArrayKey::str("name"))
    } else {
        None
    };
    let Some(Value::String(name)) = name else {
        return Err(argument.invalid("is an unsupported rounding mode"));
    };
    match name.as_bytes() {
        b"HalfAwayFromZero" => Ok(Rounding::HalfAway),
        b"HalfTowardsZero" => Ok(Rounding::HalfToward),
        b"HalfEven" => Ok(Rounding::HalfEven),
        b"HalfOdd" => Ok(Rounding::HalfOdd),
        b"TowardsZero" => Ok(Rounding::Toward),
        b"AwayFromZero" => Ok(Rounding::Away),
        b"NegativeInfinity" => Ok(Rounding::Floor),
        b"PositiveInfinity" => Ok(Rounding::Ceiling),
        _ => Err(argument.invalid("is an unsupported rounding mode")),
    }
}

pub(crate) fn expanded(scale: u32, extra: u32) -> Result<u32> {
    if scale <= i32::MAX as u32 && extra <= i32::MAX as u32 - scale {
        Ok(scale + extra)
    } else {
        Err(NativeError::value_error("scale of the result is too large"))
    }
}

pub(crate) fn auto_expand(result: Decimal, original_scale: u32) -> Decimal {
    result.resized(original_scale.max(result.compact_scale()))
}

pub(crate) fn calculate(
    operation: Operation,
    left: &Decimal,
    right: &Decimal,
    scale: Option<u32>,
    exponent_argument: Option<Argument>,
) -> Result<Decimal> {
    let common = scale.unwrap_or(left.scale.max(right.scale));
    Ok(match operation {
        Operation::Add | Operation::Sub => left
            .add(right, matches!(operation, Operation::Sub))
            .resized(common),
        Operation::Mul => {
            let scale = match scale {
                Some(scale) => scale,
                None => expanded(left.scale, right.scale)?,
            };
            left.multiply(right).resized(scale)
        }
        Operation::Div => {
            nonzero(right, "Division")?;
            if let Some(scale) = scale {
                left.divide(right, scale)
            } else {
                auto_expand(left.divide(right, expanded(left.scale, 10)?), left.scale)
            }
        }
        Operation::Mod => {
            nonzero(right, "Modulo")?;
            left.remainder(right, common)
        }
        Operation::Pow => power(left, right, scale, exponent_argument)?,
    })
}

fn power(
    base: &Decimal,
    exponent: &Decimal,
    scale: Option<u32>,
    argument: Option<Argument>,
) -> Result<Decimal> {
    let error = |detail: &str| match argument {
        Some(argument) => argument.invalid(detail),
        None => NativeError::value_error(detail),
    };
    if exponent.fraction() {
        return Err(error(
            if argument.is_some_and(|argument| argument.0 == "bcpow") {
                "cannot have a fractional part"
            } else {
                "exponent cannot have a fractional part"
            },
        ));
    }
    let Some(exponent) = exponent
        .at_scale(0)
        .to_i64()
        .filter(|value| *value != i64::MIN)
    else {
        return Err(error(
            if argument.is_some_and(|argument| argument.0 == "bcpow") {
                "is too large"
            } else {
                "exponent is too large"
            },
        ));
    };
    let automatic = scale.is_none();
    let magnitude = exponent.unsigned_abs();
    let scale = match scale {
        Some(scale) => scale,
        None if exponent < 0 => expanded(base.scale, 10)?,
        None => {
            let determined = u64::from(base.scale).saturating_mul(magnitude);
            if determined > i32::MAX as u64 {
                return Err(NativeError::value_error("scale of the result is too large"));
            }
            determined as u32
        }
    };
    let compact = base.resized(base.compact_scale());
    let value = if exponent == 0 {
        Decimal::integer(1).resized(scale)
    } else if compact.coefficient.is_zero() {
        if exponent < 0 {
            return Err(NativeError::throw(
                "DivisionByZeroError",
                "Negative power of zero",
            ));
        }
        Decimal::integer(0).resized(scale)
    } else if compact.scale == 0 && compact.coefficient.abs().is_one() {
        Decimal::integer(if exponent.is_even() {
            BigInt::one()
        } else {
            compact.coefficient
        })
        .resized(scale)
    } else {
        let digits = compact.coefficient.magnitude().to_str_radix(10).len() as u64;
        if magnitude > i32::MAX as u64 / digits.max(u64::from(compact.scale)) {
            return Err(error(
                "exponent is too large, the number of digits overflowed",
            ));
        }
        let power = Decimal {
            coefficient: compact.coefficient.pow(magnitude as u32),
            scale: compact.scale * magnitude as u32,
        };
        if exponent < 0 {
            Decimal::integer(1).divide(&power, scale)
        } else {
            power.resized(scale)
        }
    };
    Ok(if automatic && exponent < 0 {
        auto_expand(value, base.scale)
    } else {
        value
    })
}

pub(crate) fn sqrt(value: &Decimal, scale: Option<u32>, negative: NativeError) -> Result<Decimal> {
    if value.coefficient.is_negative() {
        return Err(negative);
    }
    Ok(if let Some(scale) = scale {
        value.sqrt(scale)
    } else {
        auto_expand(value.sqrt(expanded(value.scale, 10)?), value.scale)
    })
}

pub(crate) fn powmod(
    base: &Decimal,
    exponent: &Decimal,
    modulus: &Decimal,
    scale: u32,
    arguments: [Option<Argument>; 3],
) -> Result<Decimal> {
    for (number, argument) in [(base, arguments[0]), (exponent, arguments[1])] {
        if number.fraction() {
            return Err(argument.map_or_else(
                || NativeError::value_error("Base number cannot have a fractional part"),
                |argument| argument.invalid("cannot have a fractional part"),
            ));
        }
    }
    if exponent.coefficient.is_negative()
        && let Some(argument) = arguments[1]
    {
        return Err(argument.invalid("must be greater than or equal to 0"));
    }
    if modulus.fraction()
        && let Some(argument) = arguments[2]
    {
        return Err(argument.invalid("cannot have a fractional part"));
    }
    nonzero(modulus, "Modulo")?;
    let exponent = exponent.at_scale(0);
    let base = base.at_scale(0);
    let modulus = modulus.at_scale(0).abs();
    let remainder = base.abs().modpow(&exponent, &modulus);
    let remainder = if base.is_negative() && exponent.is_odd() {
        -remainder
    } else {
        remainder
    };
    Ok(Decimal::integer(remainder).resized(scale))
}
