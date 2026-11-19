//! Procedural decimal math uses the request's `bcmath.scale` unless specified.

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeResult, Param, ParamType, Value, args,
};

use crate::arithmetic::{self, Argument, Operation, Result};
use crate::decimal::{Decimal, Rounding};

const SCALE: Param = Param::new("scale", ParamType::Int)
    .nullable()
    .with_default(ConstantValue::Null);
const NUM: Param = Param::new("num", ParamType::String);
const EXPONENT: Param = Param::new("exponent", ParamType::String);
const MODULUS: Param = Param::new("modulus", ParamType::String);
const BINARY: &[Param] = &[
    Param::new("num1", ParamType::String),
    Param::new("num2", ParamType::String),
    SCALE,
];

pub(crate) const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (FunctionEntry::sync("bcadd", BINARY, add), "string"),
    (FunctionEntry::sync("bcsub", BINARY, sub), "string"),
    (FunctionEntry::sync("bcmul", BINARY, mul), "string"),
    (FunctionEntry::sync("bcdiv", BINARY, div), "string"),
    (FunctionEntry::sync("bcmod", BINARY, modulo), "string"),
    (FunctionEntry::sync("bcdivmod", BINARY, divmod), "array"),
    (FunctionEntry::sync("bccomp", BINARY, compare), "int"),
    (
        FunctionEntry::sync("bcpow", &[NUM, EXPONENT, SCALE], pow),
        "string",
    ),
    (
        FunctionEntry::sync("bcpowmod", &[NUM, EXPONENT, MODULUS, SCALE], powmod),
        "string",
    ),
    (FunctionEntry::sync("bcsqrt", &[NUM, SCALE], sqrt), "string"),
    (FunctionEntry::sync("bcscale", &[SCALE], bcscale), "int"),
    (FunctionEntry::sync("bcfloor", &[NUM], floor), "string"),
    (FunctionEntry::sync("bcceil", &[NUM], ceil), "string"),
    (
        FunctionEntry::sync(
            "bcround",
            &[
                NUM,
                Param::new("precision", ParamType::Int).with_default(ConstantValue::Int(0)),
                Param::new("mode", ParamType::Class("RoundingMode"))
                    .with_constant_default("RoundingMode::HalfAwayFromZero", ConstantValue::Null),
            ],
            round,
        ),
        "string",
    ),
];

fn default_scale(context: &dyn Context) -> i64 {
    context.ini_get(b"bcmath.scale").map_or(0, |setting| {
        phera_extension_api::ini::parse_quantity(setting.as_bytes()).0
    })
}

pub(crate) fn update_scale(
    context: &mut dyn Context,
    _: &str,
    value: Option<&[u8]>,
    _: &str,
) -> Result<bool> {
    let Some(value) = value else {
        return Ok(true);
    };
    let (scale, warning) = phera_extension_api::ini::parse_quantity(value);
    if let Some(warning) = warning {
        context.warning(format!("Invalid \"bcmath.scale\" setting. {warning}"));
    }
    Ok((0..=i64::from(i32::MAX)).contains(&scale))
}

fn scale(context: &dyn Context, values: &[Value], argument: Argument) -> Result<u32> {
    argument.scale(args::int(values, argument.1 - 1).unwrap_or_else(|| default_scale(context)))
}

fn two(
    context: &dyn Context,
    values: &[Value],
    function: &'static str,
) -> Result<(Decimal, Decimal, u32)> {
    let scale = scale(context, values, Argument(function, 3, "scale"))?;
    let left = Argument(function, 1, "num1").parse(args::bytes(values, 0))?;
    let right = Argument(function, 2, "num2").parse(args::bytes(values, 1))?;
    Ok((left, right, scale))
}

fn binary(
    context: &dyn Context,
    values: &[Value],
    function: &'static str,
    operation: Operation,
) -> NativeResult {
    let (left, right, scale) = two(context, values, function)?;
    Ok(Value::from(
        arithmetic::calculate(operation, &left, &right, Some(scale), None)?.render(),
    ))
}

fn add(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    binary(context, values, "bcadd", Operation::Add)
}

fn sub(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    binary(context, values, "bcsub", Operation::Sub)
}

fn mul(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    binary(context, values, "bcmul", Operation::Mul)
}

fn div(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    binary(context, values, "bcdiv", Operation::Div)
}

fn modulo(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    binary(context, values, "bcmod", Operation::Mod)
}

fn divmod(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let (left, right, scale) = two(context, values, "bcdivmod")?;
    arithmetic::nonzero(&right, "Division")?;
    arithmetic::check_scale_storage(scale)?;
    Ok(crate::number::pair(
        Value::from(left.divide(&right, 0).render()),
        Value::from(left.remainder(&right, scale).render()),
    ))
}

fn compare(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let (left, right, scale) = two(context, values, "bccomp")?;
    Ok(Value::Int(left.compare(&right, scale) as i64))
}

fn pow(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let scale = scale(context, values, Argument("bcpow", 3, "scale"))?;
    let argument = Argument("bcpow", 2, "exponent");
    let left = Argument("bcpow", 1, "num").parse(args::bytes(values, 0))?;
    let right = argument.parse(args::bytes(values, 1))?;
    let result = arithmetic::calculate(Operation::Pow, &left, &right, Some(scale), Some(argument))?;
    Ok(Value::from(result.render()))
}

fn powmod(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let scale = scale(context, values, Argument("bcpowmod", 4, "scale"))?;
    let base = Argument("bcpowmod", 1, "num");
    let exponent = Argument("bcpowmod", 2, "exponent");
    let modulus = Argument("bcpowmod", 3, "modulus");
    let result = arithmetic::powmod(
        &base.parse(args::bytes(values, 0))?,
        &exponent.parse(args::bytes(values, 1))?,
        &modulus.parse(args::bytes(values, 2))?,
        scale,
        [Some(base), Some(exponent), Some(modulus)],
    )?;
    Ok(Value::from(result.render()))
}

fn sqrt(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let scale = scale(context, values, Argument("bcsqrt", 2, "scale"))?;
    let argument = Argument("bcsqrt", 1, "num");
    let value = argument.parse(args::bytes(values, 0))?;
    Ok(Value::from(
        arithmetic::sqrt(
            &value,
            Some(scale),
            argument.invalid("must be greater than or equal to 0"),
        )?
        .render(),
    ))
}

fn bcscale(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let old = default_scale(context);
    if let Some(new) = args::int(values, 0) {
        let new = Argument("bcscale", 1, "scale").scale(new)?;
        context.ini_set("bcscale", b"bcmath.scale", new.to_string().into())?;
    }
    Ok(Value::Int(old))
}

fn floor(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let number = Argument("bcfloor", 1, "num").parse(args::bytes(values, 0))?;
    Ok(Value::from(number.round(0, Rounding::Floor).render()))
}

fn ceil(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let number = Argument("bcceil", 1, "num").parse(args::bytes(values, 0))?;
    Ok(Value::from(number.round(0, Rounding::Ceiling).render()))
}

fn round(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let precision =
        Argument("bcround", 2, "precision").precision(args::int(values, 1).unwrap_or(0))?;
    let mode = arithmetic::rounding(values.get(2), Argument("bcround", 3, "mode"))?;
    let number = Argument("bcround", 1, "num").parse(args::bytes(values, 0))?;
    Ok(Value::from(
        arithmetic::round(&number, precision, mode)?.render(),
    ))
}
