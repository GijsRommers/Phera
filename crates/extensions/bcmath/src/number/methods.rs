//! Number methods retain operand precision unless given an explicit scale.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ConstantValue, Context, MethodEntry, NativeError, NativeNumericParameter,
    NativeResult, Object, Param, ParamType, PhpArray, Value, args,
};

use crate::arithmetic::{self, Argument, Operation, Result};
use crate::decimal::{Decimal, Rounding};

use super::{NAME, create, pair, readonly, receiver, value};

static NUMBER_PARAMETER: NativeNumericParameter = NativeNumericParameter {
    class: NAME,
    declaration: "BcMath\\Number|string|int",
    type_error: "int, string, or BcMath\\Number",
    string_error: None,
};
const OPERAND: ParamType = ParamType::NativeNumeric(&NUMBER_PARAMETER);
const NUM: Param = Param::new("num", OPERAND);
const SCALE: Param = Param::new("scale", ParamType::Int)
    .nullable()
    .with_default(ConstantValue::Null);

pub(super) const METHODS: &[MethodEntry] = &[
    MethodEntry::public(
        "__construct",
        &[Param::new("num", ParamType::IntOrString)],
        construct,
    ),
    MethodEntry::public("add", &[NUM, SCALE], add).returning(NAME),
    MethodEntry::public("sub", &[NUM, SCALE], sub).returning(NAME),
    MethodEntry::public("mul", &[NUM, SCALE], mul).returning(NAME),
    MethodEntry::public("div", &[NUM, SCALE], div).returning(NAME),
    MethodEntry::public("mod", &[NUM, SCALE], modulo).returning(NAME),
    MethodEntry::public("divmod", &[NUM, SCALE], divmod).returning("array"),
    MethodEntry::public("pow", &[Param::new("exponent", OPERAND), SCALE], pow).returning(NAME),
    MethodEntry::public(
        "powmod",
        &[
            Param::new("exponent", OPERAND),
            Param::new("modulus", OPERAND),
            SCALE,
        ],
        powmod,
    )
    .returning(NAME),
    MethodEntry::public("sqrt", &[SCALE], sqrt).returning(NAME),
    MethodEntry::public("ceil", &[], ceil).returning(NAME),
    MethodEntry::public("floor", &[], floor).returning(NAME),
    MethodEntry::public(
        "round",
        &[
            Param::new("precision", ParamType::Int).with_default(ConstantValue::Int(0)),
            Param::new("mode", ParamType::Class("RoundingMode")).optional(),
        ],
        round,
    )
    .returning(NAME),
    MethodEntry::public("compare", &[NUM, SCALE], compare).returning("int"),
    MethodEntry::public("__toString", &[], to_string).returning("string"),
    MethodEntry::public("__serialize", &[], serialize).returning("array"),
    MethodEntry::public(
        "__unserialize",
        &[Param::new("data", ParamType::Array)],
        unserialize,
    )
    .returning("void"),
];

fn construct(_: &mut dyn Context, this: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    let this =
        this.ok_or_else(|| NativeError::error("BcMath\\Number constructor needs an object"))?;
    if this.native.borrow().is_some() {
        return Err(readonly("value"));
    }
    let number = operand(
        args::value(values, 0),
        Argument("BcMath\\Number::__construct", 1, "num"),
    )?;
    *this.native.borrow_mut() = Some(Box::new(number));
    Ok(Value::Null)
}

fn operand(raw: &Value, argument: Argument) -> Result<Decimal> {
    match raw {
        Value::Int(integer) => Ok(Decimal::integer(*integer)),
        Value::String(string) => argument.parse(string.as_bytes()),
        Value::Object(object) => {
            value(object).ok_or_else(|| NativeError::error("BcMath\\Number is not initialized"))
        }
        _ => Err(argument.invalid("is not well-formed")),
    }
}

fn scale(values: &[Value], argument: Argument) -> Result<Option<u32>> {
    args::int(values, argument.1 - 1)
        .map(|scale| argument.scale(scale))
        .transpose()
}

fn two(
    this: Option<&Rc<Object>>,
    values: &[Value],
    function: &'static str,
    parameter: &'static str,
) -> Result<(Decimal, Decimal, Option<u32>)> {
    let left = receiver(this)?;
    let right = operand(args::value(values, 0), Argument(function, 1, parameter))?;
    let scale = scale(values, Argument(function, 2, "scale"))?;
    Ok((left, right, scale))
}

fn binary(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    values: &[Value],
    function: &'static str,
    operation: Operation,
) -> NativeResult {
    let parameter = if matches!(operation, Operation::Pow) {
        "exponent"
    } else {
        "num"
    };
    let (left, right, scale) = two(this, values, function, parameter)?;
    let number = arithmetic::calculate(
        operation,
        &left,
        &right,
        scale,
        Some(Argument(function, 1, parameter)),
    )?;
    create(context, number)
}

fn add(ctx: &mut dyn Context, this: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    binary(ctx, this, values, "BcMath\\Number::add", Operation::Add)
}

fn sub(ctx: &mut dyn Context, this: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    binary(ctx, this, values, "BcMath\\Number::sub", Operation::Sub)
}

fn mul(ctx: &mut dyn Context, this: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    binary(ctx, this, values, "BcMath\\Number::mul", Operation::Mul)
}

fn div(ctx: &mut dyn Context, this: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    binary(ctx, this, values, "BcMath\\Number::div", Operation::Div)
}

fn modulo(ctx: &mut dyn Context, this: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    binary(ctx, this, values, "BcMath\\Number::mod", Operation::Mod)
}

fn pow(ctx: &mut dyn Context, this: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    binary(ctx, this, values, "BcMath\\Number::pow", Operation::Pow)
}

fn divmod(context: &mut dyn Context, this: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    let (left, right, scale) = two(this, values, "BcMath\\Number::divmod", "num")?;
    arithmetic::nonzero(&right, "Division")?;
    arithmetic::check_scale_storage(scale.unwrap_or(left.scale.max(right.scale)))?;
    let quotient = create(context, left.divide(&right, 0))?;
    let remainder = left.remainder(&right, scale.unwrap_or(left.scale.max(right.scale)));
    Ok(pair(quotient, create(context, remainder)?))
}

fn powmod(context: &mut dyn Context, this: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    let function = "BcMath\\Number::powmod";
    let exponent_arg = Argument(function, 1, "exponent");
    let modulus_arg = Argument(function, 2, "modulus");
    let exponent = operand(args::value(values, 0), exponent_arg)?;
    let modulus = operand(args::value(values, 1), modulus_arg)?;
    let scale = scale(values, Argument(function, 3, "scale"))?.unwrap_or(0);
    let number = arithmetic::powmod(
        &receiver(this)?,
        &exponent,
        &modulus,
        scale,
        [None, Some(exponent_arg), Some(modulus_arg)],
    )?;
    create(context, number)
}

fn compare(_: &mut dyn Context, this: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    let (left, right, scale) = two(this, values, "BcMath\\Number::compare", "num")?;
    Ok(Value::Int(
        left.compare(&right, scale.unwrap_or(left.scale.max(right.scale))) as i64,
    ))
}

fn sqrt(context: &mut dyn Context, this: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    let scale = scale(values, Argument("BcMath\\Number::sqrt", 1, "scale"))?;
    let negative = NativeError::value_error("Base number must be greater than or equal to 0");
    create(
        context,
        arithmetic::sqrt(&receiver(this)?, scale, negative)?,
    )
}

fn ceil(context: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    create(context, receiver(this)?.round(0, Rounding::Ceiling))
}

fn floor(context: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    create(context, receiver(this)?.round(0, Rounding::Floor))
}

fn round(context: &mut dyn Context, this: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    let function = "BcMath\\Number::round";
    let precision =
        Argument(function, 1, "precision").precision(args::int(values, 0).unwrap_or(0))?;
    let rounding = arithmetic::rounding(values.get(1), Argument(function, 2, "mode"))?;
    create(
        context,
        arithmetic::round(&receiver(this)?, precision, rounding)?,
    )
}

fn to_string(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::from(receiver(this)?.render()))
}

fn serialize(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let mut properties = PhpArray::new();
    properties.insert(
        ArrayKey::str("value"),
        Value::from(receiver(this)?.render()),
    );
    Ok(Value::from(properties))
}

fn unserialize(_: &mut dyn Context, this: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    let invalid = || {
        NativeError::throw(
            "Exception",
            format!("Invalid serialization data for {NAME} object"),
        )
    };
    let this = this.ok_or_else(invalid)?;
    let raw = args::array(values, 0)
        .and_then(|data| data.get(&ArrayKey::str("value")))
        .ok_or_else(invalid)?;
    let Value::String(encoded) = &*raw.deref() else {
        return Err(invalid());
    };
    if encoded.is_empty() {
        return Err(invalid());
    }
    if this.native.borrow().is_some() {
        return Err(readonly("value"));
    }
    let number = Decimal::parse(encoded.as_bytes()).ok_or_else(invalid)?;
    *this.native.borrow_mut() = Some(Box::new(number));
    Ok(Value::Null)
}
