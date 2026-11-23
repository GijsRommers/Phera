//! Asking what a value is, and converting it: `gettype`, the `is_*` family,
//! `intval` and friends, `settype`.

use phera_extension_api::value::{cast_to_bool, is_numeric, to_array, to_float, to_int};
use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Registry,
    SyncFn, Value, args,
};

pub(crate) fn register(registry: &mut Registry) {
    const VALUE: &[Param] = &[Param::new("value", ParamType::Mixed)];
    const INTVAL: &[Param] = &[
        Param::new("value", ParamType::Mixed),
        Param::new("base", ParamType::Int).optional(),
    ];
    const SETTYPE: &[Param] = &[
        Param::new("var", ParamType::Mixed).by_ref(),
        Param::new("type", ParamType::String),
    ];

    let one_value: &[(&str, SyncFn)] = &[
        ("gettype", gettype),
        ("get_debug_type", get_debug_type),
        ("is_null", is_null),
        ("is_bool", is_bool),
        ("is_int", is_int),
        ("is_integer", is_int),
        ("is_long", is_int),
        ("is_float", is_float),
        ("is_double", is_float),
        ("is_string", is_string),
        ("is_array", is_array),
        ("is_object", is_object),
        ("is_resource", is_resource),
        ("is_numeric", is_numeric_value),
        ("is_scalar", is_scalar),
        ("is_iterable", is_iterable),
        ("is_countable", is_countable),
        ("floatval", floatval),
        ("doubleval", floatval),
        ("boolval", boolval),
    ];
    for &(name, function) in one_value {
        registry.function(FunctionEntry::sync(name, VALUE, function));
    }
    registry.function(FunctionEntry::sync("intval", INTVAL, intval));
    registry.function(FunctionEntry::asynchronous("strval", VALUE, strval));
    registry.function(FunctionEntry::asynchronous("settype", SETTYPE, settype));
}

fn gettype(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::from(args::value(arguments, 0).gettype_name()))
}

fn get_debug_type(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(match args::value(arguments, 0) {
        Value::Resource(resource) if resource.is_closed() => Value::from("resource (closed)"),
        Value::Resource(resource) => Value::from(format!("resource ({})", resource.kind)),
        other => Value::from(other.type_name()),
    })
}

/// Defines a type test that is true for the listed kinds of value.
macro_rules! type_test {
    ($name:ident, $pattern:pat) => {
        fn $name(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
            Ok(Value::Bool(matches!(args::value(arguments, 0), $pattern)))
        }
    };
}

type_test!(is_null, Value::Null);
type_test!(is_bool, Value::Bool(_));
type_test!(is_int, Value::Int(_));
type_test!(is_float, Value::Float(_));
type_test!(is_string, Value::InternedStr(_) | Value::String(_));
type_test!(is_array, Value::Array(_));
type_test!(is_object, Value::Object(_));
type_test!(
    is_scalar,
    Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::InternedStr(_) | Value::String(_)
);
fn is_iterable(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(args::value(arguments, 0).is_iterable()))
}

fn is_countable(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(match args::value(arguments, 0) {
        Value::Array(_) => true,
        Value::Object(object) => object.class.is_countable(),
        _ => false,
    }))
}

/// A closed resource is no longer a resource to `is_resource`.
fn is_resource(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(matches!(
        args::value(arguments, 0),
        Value::Resource(resource) if !resource.is_closed()
    )))
}

fn is_numeric_value(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(match args::value(arguments, 0) {
        Value::Int(_) | Value::Float(_) => true,
        Value::InternedStr(string) => is_numeric(string.as_bytes()),
        Value::String(string) => is_numeric(string.as_bytes()),
        _ => false,
    }))
}

fn intval(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let value = args::value(arguments, 0);
    let base = args::int(arguments, 1).unwrap_or(10);
    match value {
        // The base only applies to strings, and base 10 follows the ordinary
        // cast, which also understands "1e3".
        Value::InternedStr(string) if base != 10 => {
            Ok(Value::Int(parse_in_base(string.as_bytes(), base)))
        }
        Value::String(string) if base != 10 => {
            Ok(Value::Int(parse_in_base(string.as_bytes(), base)))
        }
        other => Ok(Value::Int(to_int(other, context.diagnostics())?)),
    }
}

/// C's `strtol`, which is what PHP calls: leading whitespace, a sign, an
/// optional prefix, then digits until the first one that does not fit the
/// base. Out-of-range results stop at the int limits. PHP adds the `0b`
/// prefix for binary.
fn parse_in_base(bytes: &[u8], base: i64) -> i64 {
    let mut rest = bytes;
    while let [first, tail @ ..] = rest
        && matches!(first, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
    {
        rest = tail;
    }
    let negative = match rest {
        [b'-', tail @ ..] => {
            rest = tail;
            true
        }
        [b'+', tail @ ..] => {
            rest = tail;
            false
        }
        _ => false,
    };

    let has_prefix = |letter: u8| {
        matches!(rest, [b'0', second, digit, ..]
            if second.eq_ignore_ascii_case(&letter) && digit_value(*digit) < 16)
    };
    let base = match base {
        0 | 16 if has_prefix(b'x') => {
            rest = &rest[2..];
            16
        }
        0 | 2 if matches!(rest, [b'0', b'b' | b'B', ..]) && bytes.len() > 2 => {
            rest = &rest[2..];
            2
        }
        0 if rest.first() == Some(&b'0') => 8,
        0 => 10,
        2..=36 => base as u32,
        // An invalid base makes strtol fail, which PHP reports as 0.
        _ => return 0,
    };

    // Accumulate towards the negative side: it has room for one more value
    // than the positive side.
    let mut total: i64 = 0;
    let mut overflow = false;
    for &byte in rest {
        let digit = digit_value(byte);
        if digit >= base {
            break;
        }
        match total
            .checked_mul(i64::from(base))
            .and_then(|scaled| scaled.checked_sub(i64::from(digit)))
        {
            Some(next) => total = next,
            None => overflow = true,
        }
    }
    match (negative, overflow) {
        (true, true) => i64::MIN,
        (false, true) => i64::MAX,
        (true, false) => total,
        (false, false) => total.checked_neg().unwrap_or(i64::MAX),
    }
}

/// The value of a digit in any base up to 36; 255 for anything else.
fn digit_value(byte: u8) -> u32 {
    match byte {
        b'0'..=b'9' => u32::from(byte - b'0'),
        b'a'..=b'z' => u32::from(byte - b'a') + 10,
        b'A'..=b'Z' => u32::from(byte - b'A') + 10,
        _ => 255,
    }
}

fn floatval(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let float = to_float(args::value(arguments, 0), context.diagnostics())?;
    Ok(Value::Float(float))
}

fn strval(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let string = context.stringify(args::value(&arguments, 0)).await?;
        Ok(Value::String(string))
    })
}

fn boolval(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let flag = cast_to_bool(args::value(arguments, 0), context.diagnostics());
    Ok(Value::Bool(flag))
}

fn settype(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(convert_type(context, arguments))
}

async fn convert_type(context: &mut dyn Context, arguments: Vec<Value>) -> NativeResult {
    let arguments = &arguments;
    let Some(variable) = args::reference(arguments, 0) else {
        return Err(NativeError::error(
            "settype(): Argument #1 ($var) could not be passed by reference",
        ));
    };
    let name = args::bytes(arguments, 1).to_ascii_lowercase();
    let current = variable.borrow().clone();
    let converted = match name.as_slice() {
        b"int" | b"integer" => Value::Int(to_int(&current, context.diagnostics())?),
        b"float" | b"double" => Value::Float(to_float(&current, context.diagnostics())?),
        b"string" => Value::String(context.stringify(&current).await?),
        b"array" => Value::from(to_array(&current, context.diagnostics())),
        b"bool" | b"boolean" => Value::Bool(cast_to_bool(&current, context.diagnostics())),
        b"null" => {
            if matches!(current, Value::Float(float) if float.is_nan()) {
                context.warning("unexpected NAN value was coerced to null".to_string());
            }
            Value::Null
        }
        b"object" if current.is_object() => current,
        b"object" => {
            return Err(NativeError::error(
                "settype(): Converting to object is not supported yet",
            ));
        }
        b"resource" => {
            return Err(NativeError::value_error("Cannot convert to resource type"));
        }
        _ => {
            return Err(NativeError::value_error(
                "settype(): Argument #2 ($type) must be a valid type",
            ));
        }
    };
    *variable.borrow_mut() = converted;
    Ok(Value::Bool(true))
}
