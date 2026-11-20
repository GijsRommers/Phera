//! Coerce XML text in PHP numeric expressions and compare wrappers by node identity.
//!
//! ```php
//! echo simplexml_load_string('<r>1.25</r>') + 2; // 3.25
//! ```

use phera_extension_api::{
    Context, Diagnostics, NativeBinaryOp as Binary, NativeComparison, NativeError,
    NativeUnaryOp as Unary, Object, Value,
    value::{self, NoDiagnostics, NumVal, ObjectCast, OpError},
};

use crate::selection::{CLASS, data};

pub fn cast(object: &Object, kind: ObjectCast) -> Option<Value> {
    let state = data(object).ok()?;
    let selection = &state.selection;
    let value = match kind {
        ObjectCast::String if object.class.name() == CLASS => Value::from(selection.value()),
        ObjectCast::String => return None,
        ObjectCast::Bool => Value::Bool(selection.is_truthy()),
        ObjectCast::Int => {
            let text = Value::from(selection.value());
            Value::Int(value::to_int(&text, &mut NoDiagnostics).ok()?)
        }
        ObjectCast::Float => {
            let text = Value::from(selection.value());
            Value::Float(value::to_float(&text, &mut NoDiagnostics).ok()?)
        }
        ObjectCast::Number => match value::parse_numeric(selection.value().as_bytes()).value {
            Some(NumVal::Int(number)) => Value::Int(number),
            Some(NumVal::Float(number)) => Value::Float(number),
            None => Value::Int(0),
        },
    };
    Some(value)
}

pub fn compare(
    left: &Object,
    right: &Object,
    _: &mut dyn Diagnostics,
) -> Result<NativeComparison, OpError> {
    if !left.class.is_instance_of(CLASS) || !right.class.is_instance_of(CLASS) {
        return Ok(NativeComparison::Uncomparable);
    }
    match (data(left), data(right)) {
        (Ok(left), Ok(right)) if left.selection.anchor.is_same_node(&right.selection.anchor) => {
            Ok(NativeComparison::Ordered(std::cmp::Ordering::Equal))
        }
        _ => Ok(NativeComparison::Uncomparable),
    }
}

type NumericOperation = fn(&Value, &Value, &mut dyn Diagnostics) -> Result<Value, OpError>;

pub fn binary(
    context: &mut dyn Context,
    operator: Binary,
    left: &Value,
    right: &Value,
) -> Result<Option<Value>, NativeError> {
    let (operation, sigil): (NumericOperation, _) = match operator {
        Binary::Add => (value::add, "+"),
        Binary::Subtract => (value::sub, "-"),
        Binary::Multiply => (value::mul, "*"),
        Binary::Divide => (value::div, "/"),
        Binary::Modulo => (value::modulo, "%"),
        Binary::Power => (value::pow, "**"),
        Binary::BitwiseAnd => (value::bitwise_and, "&"),
        Binary::BitwiseOr => (value::bitwise_or, "|"),
        Binary::BitwiseXor => (value::bitwise_xor, "^"),
        Binary::ShiftLeft => (value::shift_left, "<<"),
        Binary::ShiftRight => (value::shift_right, ">>"),
        _ => return Ok(None),
    };
    let convert = |value: &Value| match value {
        Value::Object(object) if object.class.is_instance_of(CLASS) => {
            cast(object, ObjectCast::Number)
        }
        _ => Some(value.clone()),
    };
    let (Some(first), Some(second)) = (convert(left), convert(right)) else {
        return Ok(None);
    };
    if first.is_object() || second.is_object() {
        return Ok(None);
    }
    operation(&first, &second, context.diagnostics())
        .map(Some)
        .map_err(|error| {
            if error.kind == value::ErrorKind::TypeError {
                value::binop_type_error(sigil, left, right).into()
            } else {
                error.into()
            }
        })
}

pub fn unary(
    context: &mut dyn Context,
    operator: Unary,
    object: &Object,
) -> Result<Option<Value>, NativeError> {
    let Some(number) = cast(object, ObjectCast::Number) else {
        return Ok(None);
    };
    match operator {
        Unary::Plus => Ok(Some(number)),
        Unary::Negate => Ok(Some(value::negate(&number, context.diagnostics())?)),
        _ => Ok(None),
    }
}
