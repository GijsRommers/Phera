//! GMP bits use two's complement, so `gmp_testbit(-1, 1000)` is true.

use num_bigint::BigInt;
use num_traits::Signed;
use phera_extension_api::{Context, NativeResult, Value, args};

use crate::{
    arithmetic,
    integer::{self, Argument, Result},
    object,
};

fn index(values: &[Value], function: &'static str, name: &'static str) -> Result<u64> {
    let index = args::int(values, 1).unwrap_or(0);
    if (0..i64::from(i32::MAX) * 64).contains(&index) {
        Ok(index as u64)
    } else {
        Err(Argument(function, 2, name).value_error("must be between 0 and 2147483647 * 64"))
    }
}

pub(crate) fn mutate(
    _: &mut dyn Context,
    values: &[Value],
    function: &'static str,
    set: bool,
) -> NativeResult {
    let index = index(values, function, "index")?;
    if let Some(object) = args::object(values, 0) {
        let mut integer = object::value(object);
        if integer.bit(index) != set {
            arithmetic::check_bits(index + 1)?;
            integer.set_bit(index, set);
            object::set(object, integer);
        }
    }
    Ok(Value::Null)
}

pub(crate) fn test(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let number = integer::unary("gmp_testbit", values)?;
    Ok(Value::Bool(number.bit(index(
        values,
        "gmp_testbit",
        "index",
    )?)))
}

pub(crate) fn scan(
    _: &mut dyn Context,
    values: &[Value],
    function: &'static str,
    set: bool,
) -> NativeResult {
    let number = Argument(function, 1, "num1").read(values)?;
    let from = index(values, function, "start")?;
    let target = if set { number } else { !number };
    let found = (target >> from).trailing_zeros();
    Ok(Value::Int(
        found.map_or(-1, |offset| (from + offset) as i64),
    ))
}

fn count(value: &BigInt) -> i64 {
    if value.is_negative() {
        -1
    } else {
        value
            .iter_u64_digits()
            .map(|digit| i64::from(digit.count_ones()))
            .sum()
    }
}

pub(crate) fn popcount(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    Ok(Value::Int(count(&integer::unary("gmp_popcount", values)?)))
}

pub(crate) fn distance(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let (left, right) = integer::pair("gmp_hamdist", values)?;
    Ok(Value::Int(count(&(left ^ right))))
}
