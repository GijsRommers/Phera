//! Explicit PHP integer and string conversion entry points.

use num_bigint::BigInt;
use phera_extension_api::{Context, NativeResult, Value, args};

use crate::{
    integer::{self, Argument},
    object,
};

pub(super) fn init(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let number = Argument("gmp_init", 1, "num")
        .init(args::value(values, 0), args::int(values, 1).unwrap_or(0))?;
    object::create(context, number)
}

pub(super) fn intval(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    Ok(Value::Int(integer::to_int(&integer::unary(
        "gmp_intval",
        values,
    )?)))
}

pub(super) fn strval(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let number = integer::unary("gmp_strval", values)?;
    let base = args::int(values, 1).unwrap_or(10);
    if !(2..=62).contains(&base) && !(-36..=-2).contains(&base) {
        return Err(Argument("gmp_strval", 2, "base")
            .value_error("must be between 2 and 62, or -2 and -36"));
    }
    Ok(Value::from(integer::render(&number, base)))
}

pub(super) fn compare(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let (left, right) = integer::pair("gmp_cmp", values)?;
    Ok(Value::Int(left.cmp(&right) as i64))
}

pub(super) fn sign(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    Ok(Value::Int(
        integer::unary("gmp_sign", values)?.cmp(&BigInt::ZERO) as i64,
    ))
}
