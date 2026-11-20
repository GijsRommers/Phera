//! GMP arithmetic and division. `gmp_div_q(-7, 2, GMP_ROUND_MINUSINF)` is -4.

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, Zero};
use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use crate::{
    integer::{self, Argument, Result},
    object,
};

pub(crate) fn unary(
    context: &mut dyn Context,
    values: &[Value],
    name: &'static str,
    operation: impl FnOnce(BigInt) -> BigInt,
) -> NativeResult {
    object::create(context, operation(integer::unary(name, values)?))
}

pub(crate) fn binary(
    context: &mut dyn Context,
    values: &[Value],
    name: &'static str,
    operation: impl FnOnce(BigInt, BigInt) -> BigInt,
) -> NativeResult {
    let (left, right) = integer::pair(name, values)?;
    object::create(context, operation(left, right))
}

pub(crate) fn zero(operation: &str) -> NativeError {
    NativeError::throw("DivisionByZeroError", format!("{operation} by zero"))
}

fn check_divisor(divisor: &BigInt, name: &str, message: &str) -> Result<()> {
    if divisor.is_zero() {
        Err(NativeError::throw(
            "DivisionByZeroError",
            format!("{name}(): Argument #2 ($num2) {message} by zero"),
        ))
    } else {
        Ok(())
    }
}

pub(crate) enum Division {
    Quotient,
    Remainder,
    Both,
}

pub(crate) fn divide(
    context: &mut dyn Context,
    values: &[Value],
    name: &'static str,
    result: Division,
) -> NativeResult {
    let (left, right) = integer::pair(name, values)?;
    check_divisor(&right, name, "Division")?;
    let quotient = match args::int(values, 2).unwrap_or(0) {
        0 => &left / &right,
        1 => left.div_ceil(&right),
        2 => left.div_floor(&right),
        _ => {
            return Err(Argument(name, 3, "rounding_mode").value_error(
                "must be one of GMP_ROUND_ZERO, GMP_ROUND_PLUSINF, or GMP_ROUND_MINUSINF",
            ));
        }
    };
    match result {
        Division::Quotient => object::create(context, quotient),
        Division::Remainder => object::create(context, left - quotient * right),
        Division::Both => {
            let remainder = left - &quotient * right;
            object::pair(context, quotient, remainder)
        }
    }
}

pub(crate) fn modulo(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let (left, right) = integer::pair("gmp_mod", values)?;
    check_divisor(&right, "gmp_mod", "Modulo")?;
    object::create(context, left.mod_floor(&right.abs()))
}

pub(crate) fn divexact(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let (left, right) = integer::pair("gmp_divexact", values)?;
    check_divisor(&right, "gmp_divexact", "Division")?;
    object::create(context, left / right)
}

// Reject work that cannot produce a representable GMP result within the
// extension's maximum bit budget, before asking BigInt to allocate it.
pub(crate) fn check_bits(bits: u64) -> Result<()> {
    if bits > i32::MAX as u64 {
        Err(NativeError::value_error("GMP integer is too large"))
    } else {
        Ok(())
    }
}

pub(crate) fn power(base: BigInt, exponent: u64) -> Result<BigInt> {
    if exponent == 0 {
        return Ok(BigInt::one());
    }
    if base.is_zero() || base.abs().is_one() {
        return Ok(if exponent.is_even() { base.abs() } else { base });
    }
    check_bits(base.bits().saturating_mul(exponent))?;
    Ok(base.pow(exponent as u32))
}

pub(crate) fn pow(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let base = integer::unary("gmp_pow", values)?;
    let exponent =
        Argument("gmp_pow", 2, "exponent").nonnegative(args::int(values, 1).unwrap_or(0))?;
    object::create(context, power(base, exponent)?)
}

pub(crate) fn powmod(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let base = integer::unary("gmp_powm", values)?;
    let exponent = Argument("gmp_powm", 2, "exponent").read(values)?;
    let modulus = Argument("gmp_powm", 3, "modulus").read(values)?.abs();
    if exponent.is_negative() {
        return Err(
            Argument("gmp_powm", 2, "exponent").value_error("must be greater than or equal to 0")
        );
    }
    if modulus.is_zero() {
        return Err(zero("Modulo"));
    }
    object::create(context, base.modpow(&exponent, &modulus))
}

pub(crate) fn factorial(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let input = integer::unary("gmp_fact", values)?;
    if input.is_negative() {
        return Err(
            Argument("gmp_fact", 1, "num").value_error("must be greater than or equal to 0")
        );
    }
    let count = input.iter_u64_digits().next().unwrap_or(0);
    check_bits(count.saturating_mul(u64::from(u64::BITS - count.leading_zeros())))?;
    let result = (2..=count).fold(BigInt::one(), |product, value| product * value);
    object::create(context, result)
}

pub(crate) fn binomial(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let number = Argument("gmp_binomial", 1, "n").read(values)?;
    let count = Argument("gmp_binomial", 2, "k").nonnegative(args::int(values, 1).unwrap_or(0))?;
    let (number, negative) = if number.is_negative() {
        (BigInt::from(count) - &number - 1u32, count.is_odd())
    } else {
        (number, false)
    };
    let complement = &number - count;
    if complement.is_negative() {
        return object::create(context, BigInt::ZERO);
    }
    use num_traits::ToPrimitive;
    let count = count.min(complement.to_u64().unwrap_or(u64::MAX));
    check_bits(number.bits().saturating_mul(count))?;
    let result = (0..count).fold(BigInt::one(), |product, index| {
        product * (&number - index) / (index + 1)
    });
    object::create(context, if negative { -result } else { result })
}

pub(crate) fn sqrt(
    context: &mut dyn Context,
    values: &[Value],
    function: &'static str,
    remainder: bool,
) -> NativeResult {
    let number = integer::unary(function, values)?;
    if number.is_negative() {
        return Err(Argument(function, 1, "num").value_error("must be greater than or equal to 0"));
    }
    let root = number.sqrt();
    if remainder {
        let remainder = number - &root * &root;
        object::pair(context, root, remainder)
    } else {
        object::create(context, root)
    }
}

pub(crate) fn root(
    context: &mut dyn Context,
    values: &[Value],
    function: &'static str,
    remainder: bool,
) -> NativeResult {
    let number = integer::unary(function, values)?;
    let nth = args::int(values, 1).unwrap_or(0);
    let argument = Argument(function, 2, "nth");
    if nth <= 0 {
        let message = if remainder {
            "must be greater than or equal to 1"
        } else {
            "must be greater than 0"
        };
        return Err(argument.value_error(message));
    }
    if number.is_negative() && nth.is_even() {
        return Err(argument.value_error("must be odd if argument #1 ($a) is negative"));
    }
    let root = if let Ok(nth) = u32::try_from(nth) {
        number.nth_root(nth)
    } else {
        number.signum()
    };
    if remainder {
        let rest = number - power(root.clone(), nth as u64)?;
        object::pair(context, root, rest)
    } else {
        object::create(context, root)
    }
}
