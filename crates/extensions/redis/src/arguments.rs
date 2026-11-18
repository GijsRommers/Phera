//! Methods whose arguments phpredis's C code checks by hand, so that PHP's
//! answer differs from what their declared signature would give:
//!
//! ```php
//! $redis->bitop('AND', 'destination');   // false, not an ArgumentCountError
//! $redis->del(new stdClass());           // Error: Object of class stdClass could not be converted to string
//! $redis->delete('key');                 // Deprecated: Method Redis::delete() is deprecated
//! ```
//!
//! Each function is the preparation hook (see `MethodEntry::with_preparation`)
//! of the methods that the registration attaches it to; every other method's
//! arguments are checked against its declared signature.

use phera_extension_api::value::to_php_string;
use phera_extension_api::{Context, MethodPreparation, NativeError, Param, Value, prepare_call};

type Preparation = Result<MethodPreparation, NativeError>;

/// The deprecated aliases (`delete`, `open`, `sortAsc` ...) warn first.
pub fn deprecated(name: &str, context: &mut dyn Context) {
    context.deprecated(format!("Method {name}() is deprecated"));
}

/// Checks the arguments against `parsed`, the signature phpredis's parser
/// really uses, instead of the declared one.
pub fn parse_as(
    name: &str,
    parsed: &[Param],
    context: &mut dyn Context,
    arguments: &mut [Value],
) -> Preparation {
    prepare_call(name, parsed, arguments, context)?;
    Ok(MethodPreparation::Prepared)
}

/// A list of keys (`del`, `exists`, `blPop` ...): any value is a key, and an
/// object in first place must be convertible to a string.
pub fn key_list(
    name: &str,
    parsed: &[Param],
    context: &mut dyn Context,
    arguments: &mut [Value],
) -> Preparation {
    prepare_call(name, parsed, arguments, context)?;
    if let Some(object @ Value::Object(_)) = arguments.first() {
        to_php_string(object, context.diagnostics())?;
    }
    Ok(MethodPreparation::Prepared)
}

/// `command()`, `slaveof()` and `replicaof()` accept null, but not an object
/// where null may go.
pub fn object_not_null(
    name: &str,
    parsed: &[Param],
    context: &mut dyn Context,
    arguments: &mut [Value],
) -> Preparation {
    if matches!(arguments.first(), Some(Value::Object(_))) {
        parse_as(name, parsed, context, arguments)
    } else {
        Ok(MethodPreparation::Default)
    }
}

/// `geoAdd()`, `sPop()`, `zPopMin()` and `zPopMax()` with no argument at all.
pub fn not_empty(name: &str, arguments: &[Value]) -> Result<(), NativeError> {
    if arguments.is_empty() {
        return Err(NativeError::argument_count_error(format!(
            "Wrong parameter count for {name}()"
        )));
    }
    Ok(())
}

/// `hSet()`, `lPush()`, `rPush()` and `zAdd()` take their values as variadic
/// arguments but need at least `required` arguments in all.
pub fn at_least(name: &str, required: usize, arguments: &[Value]) -> Preparation {
    if arguments.len() < required {
        return Err(NativeError::argument_count_error(format!(
            "{name}() expects at least {required} arguments, {} given",
            arguments.len()
        )));
    }
    Ok(MethodPreparation::Default)
}

/// `bitop()` returns false for too few operands or an object among them.
pub fn bitop(arguments: &[Value]) -> Preparation {
    if arguments.len() < 3
        || arguments
            .iter()
            .any(|value| matches!(value, Value::Object(_)))
    {
        return Ok(MethodPreparation::Return(Value::Bool(false)));
    }
    Ok(MethodPreparation::Default)
}

/// `zRangeByLex()` and `zRevRangeByLex()`: a range, or a range with LIMIT.
pub fn three_or_five(name: &str, context: &mut dyn Context, arguments: &[Value]) -> Preparation {
    if !matches!(arguments.len(), 3 | 5) {
        context.warning(format!("{name}(): Must pass either 3 or 5 arguments"));
        return Ok(MethodPreparation::Return(Value::Bool(false)));
    }
    Ok(MethodPreparation::Default)
}

pub fn raw_command(name: &str, context: &mut dyn Context, arguments: &[Value]) -> Preparation {
    if arguments
        .first()
        .is_some_and(|value| !matches!(value, Value::String(_)))
    {
        context.warning(format!(
            "{name}(): When sending a 'raw' command, the first argument must be a string!"
        ));
        return Ok(MethodPreparation::Return(Value::Bool(false)));
    }
    Ok(MethodPreparation::Default)
}

/// `script()` returns false, without a warning, unless the subcommand is a string.
pub fn script(arguments: &[Value]) -> Preparation {
    if arguments
        .first()
        .is_some_and(|value| !matches!(value, Value::String(_)))
    {
        return Ok(MethodPreparation::Return(Value::Bool(false)));
    }
    Ok(MethodPreparation::Default)
}

pub fn pfcount(name: &str, context: &mut dyn Context, arguments: &[Value]) -> Preparation {
    if arguments
        .first()
        .is_some_and(|value| !matches!(value, Value::String(_) | Value::Array(_)))
    {
        context.warning(format!(
            "{name}(): Argument must be either an array or a string"
        ));
        return Ok(MethodPreparation::Return(Value::Bool(false)));
    }
    Ok(MethodPreparation::Default)
}
