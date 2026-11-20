//! Number-theoretic operations. `gmp_invert(3, 7)` gives 5.

use num_bigint::{BigInt, BigUint};
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};
use phera_extension_api::{ArrayKey, Context, NativeResult, PhpArray, Value, args};

use crate::{arithmetic, integer, object};

pub(crate) fn gcdext(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let (left, right) = integer::pair("gmp_gcdext", values)?;
    let result = left.extended_gcd(&right);
    let (mut s, mut t) = (result.x, result.y);
    if result.gcd.is_zero() {
        s = BigInt::ZERO;
        t = BigInt::ZERO;
    } else if left.abs() == right.abs() {
        s = BigInt::ZERO;
        t = right.signum();
    }
    let mut array = PhpArray::new();
    for (key, value) in [("g", result.gcd), ("s", s), ("t", t)] {
        array.insert(ArrayKey::str(key), object::create(context, value)?);
    }
    Ok(Value::from(array))
}

pub(crate) fn invert(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let (left, right) = integer::pair("gmp_invert", values)?;
    if right.is_zero() {
        return Err(arithmetic::zero("Division"));
    }
    if right.abs().is_one() {
        return object::create(context, BigInt::ZERO);
    }
    match left.modinv(&right.abs()) {
        Some(inverse) => object::create(context, inverse),
        None => Ok(Value::Bool(false)),
    }
}

pub(crate) fn square(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let number = integer::unary("gmp_perfect_square", values)?;
    if number.is_negative() {
        return Ok(Value::Bool(false));
    }
    let root = number.sqrt();
    Ok(Value::Bool(&root * &root == number))
}

pub(crate) fn perfect_power(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let number = integer::unary("gmp_perfect_power", values)?;
    let magnitude = number.magnitude();
    if magnitude.bits() <= 1 {
        return Ok(Value::Bool(true));
    }
    let start = if number.is_negative() { 3 } else { 2 };
    for exponent in start..=magnitude.bits().min(u64::from(u32::MAX)) as u32 {
        if (number.is_negative() && exponent.is_even())
            || (2..)
                .take_while(|divisor| divisor <= &(exponent / divisor))
                .any(|divisor| exponent % divisor == 0)
        {
            continue;
        }
        if magnitude.nth_root(exponent).pow(exponent) == *magnitude {
            return Ok(Value::Bool(true));
        }
    }
    Ok(Value::Bool(false))
}

pub(crate) fn symbol(
    _: &mut dyn Context,
    values: &[Value],
    function: &'static str,
) -> NativeResult {
    let (left, right) = integer::pair(function, values)?;
    Ok(Value::Int(kronecker(left, right)))
}

fn kronecker(mut numerator: BigInt, mut denominator: BigInt) -> i64 {
    if denominator.is_zero() {
        return i64::from(numerator.abs().is_one());
    }
    if numerator.is_zero() {
        return i64::from(denominator.abs().is_one());
    }
    let mut sign = 1;
    if denominator.is_negative() {
        denominator = -denominator;
        if numerator.is_negative() {
            sign = -sign;
        }
    }
    let twos = denominator.trailing_zeros().unwrap_or(0);
    if twos != 0 {
        if numerator.is_even() {
            return 0;
        }
        if twos.is_odd() && matches!(numerator.mod_floor(&8.into()).to_u32(), Some(3 | 5)) {
            sign = -sign;
        }
        denominator >>= twos;
    }
    numerator = numerator.mod_floor(&denominator);
    while !numerator.is_zero() {
        let twos = numerator.trailing_zeros().unwrap_or(0);
        numerator >>= twos;
        if twos.is_odd() && matches!(denominator.mod_floor(&8.into()).to_u32(), Some(3 | 5)) {
            sign = -sign;
        }
        if numerator.bit(1) && denominator.bit(1) {
            sign = -sign;
        }
        std::mem::swap(&mut numerator, &mut denominator);
        numerator %= &denominator;
    }
    if denominator.is_one() { sign } else { 0 }
}

const SMALL_PRIMES: &[u64] = &[
    2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61,
];
const DETERMINISTIC_BASES: &[u64] = &[2, 325, 9375, 28178, 450775, 9780504, 1795265022];

pub(crate) fn prob_prime(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let number = integer::unary("gmp_prob_prime", values)?;
    let repetitions = args::int(values, 1).unwrap_or(10) as i32;
    Ok(Value::Int(if prime(number.magnitude(), repetitions) {
        if number.magnitude() < &BigUint::from(31u64 << 46) {
            2
        } else {
            1
        }
    } else {
        0
    }))
}

pub(crate) fn next_prime(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let number = integer::unary("gmp_nextprime", values)?;
    if number < BigInt::from(2) {
        return object::create(context, BigInt::from(2));
    }
    let mut candidate = number.magnitude() + if number.is_even() { 1u8 } else { 2u8 };
    while !prime(&candidate, 25) {
        candidate += 2u8;
    }
    object::create(context, candidate.into())
}

fn prime(number: &BigUint, repetitions: i32) -> bool {
    if number < &BigUint::from(2u8) {
        return false;
    }
    for prime in SMALL_PRIMES {
        if number % *prime == BigUint::ZERO {
            return number == &BigUint::from(*prime);
        }
    }
    let previous = number - 1u8;
    let twos = previous.trailing_zeros().unwrap_or(0);
    let odd = &previous >> twos;
    let rounds = repetitions.clamp(16, 64) as u64;
    let witnesses = if number.bits() <= 64 {
        DETERMINISTIC_BASES
            .iter()
            .copied()
            .chain(0..0)
            .take(DETERMINISTIC_BASES.len())
    } else {
        DETERMINISTIC_BASES
            .iter()
            .copied()
            .chain(63..63 + rounds)
            .take(rounds as usize)
    };
    for base in witnesses {
        let base = BigUint::from(base) % number;
        if base.is_zero() {
            continue;
        }
        let mut power = base.modpow(&odd, number);
        if power.is_one() || power == previous {
            continue;
        }
        let mut composite = true;
        for _ in 1..twos {
            power = (&power * &power) % number;
            if power == previous {
                composite = false;
                break;
            }
        }
        if composite {
            return false;
        }
    }
    true
}
