//! Checks and converts the arguments of a native call before the function
//! body runs, the way PHP's own parameter parsing does.
//!
//! The VM calls [`prepare_arguments`] once per native call. After it returns
//! `Ok`, a function body may rely on these guarantees and needs no checks of
//! its own:
//!
//! - The number of arguments is within what the signature allows.
//! - An argument for a `Bool`, `Int`, `Float`, `String` or `Array` parameter
//!   holds exactly that kind of [`Value`]; `Number` holds an int or a float,
//!   `IntOrString` an int or a string, `ArrayOrString` an array or a string.
//! - An argument for a nullable parameter may also be `Value::Null`.
//! - `Mixed` arguments are passed through untouched.
//! - An argument for a by-reference parameter is a `Value::Reference`.
//! - No other argument is a `Value::Reference`.
//!
//! Optional parameters that the caller left out are simply missing from the
//! end of the slice. The [`args`] helpers read arguments under these
//! guarantees.

use phera_value::{
    NumVal, PhpString, Value, cast_to_bool, float_repr, parse_numeric, to_php_string,
};

use crate::{Context, FunctionEntry, NativeError, Param, ParamType};

/// Validates `arguments` against the signature of `entry` and converts scalar
/// arguments in place. On failure the error is the exception PHP would throw
/// (`ArgumentCountError` or `TypeError`), with PHP's message.
pub fn prepare_arguments(
    entry: &FunctionEntry,
    arguments: &mut [Value],
    context: &mut dyn Context,
) -> Result<(), NativeError> {
    prepare_call(entry.name, entry.params, arguments, context)
}

/// [`prepare_arguments`] for anything with a parameter list. For a native
/// method, `name` is `Class::method`, which is how PHP names it in errors.
pub fn prepare_call(
    name: &str,
    params: &[Param],
    arguments: &mut [Value],
    context: &mut dyn Context,
) -> Result<(), NativeError> {
    check_count(name, params, arguments.len())?;
    let strict = context.strict_types();
    for (index, argument) in arguments.iter_mut().enumerate() {
        // Arguments past the declared list belong to the variadic parameter.
        // `check_count` has already rejected them when there is none.
        let Some(param) = params.get(index).or(params.last()) else {
            break;
        };
        prepare_argument(name, index, param, argument, strict, context)?;
    }
    Ok(())
}

/// Prepare one supplied argument when the VM parses a list that may call PHP.
pub fn prepare_single_argument(
    name: &str,
    index: usize,
    param: &Param,
    argument: &mut Value,
    context: &mut dyn Context,
) -> Result<(), NativeError> {
    prepare_argument(
        name,
        index,
        param,
        argument,
        context.strict_types(),
        context,
    )
}

#[inline]
fn prepare_argument(
    name: &str,
    index: usize,
    param: &Param,
    argument: &mut Value,
    strict: bool,
    context: &mut dyn Context,
) -> Result<(), NativeError> {
    let site = Site {
        function: name,
        number: index + 1,
        param,
    };
    if param.by_ref {
        // The function sees the caller's variable, so the conversion has
        // to happen inside the reference.
        if let Value::Reference(cell) = argument {
            let mut target = cell.try_borrow_mut().map_err(|_| {
                NativeError::error(format!(
                    "{}(): Argument #{} is already being modified",
                    site.function, site.number
                ))
            })?;
            convert(&mut target, &site, strict, context)?;
        }
        return Ok(());
    }
    if argument.is_reference() {
        let plain = argument.deref().clone();
        *argument = plain;
    }
    convert(argument, &site, strict, context)?;
    Ok(())
}

/// Where an argument sits, for error messages.
struct Site<'a> {
    function: &'a str,
    /// Counted from 1, as PHP prints it.
    number: usize,
    param: &'a Param,
}

impl Site<'_> {
    /// ` ($name)`, which PHP leaves out for the arguments of a variadic
    /// parameter.
    fn name(&self) -> String {
        if self.param.variadic {
            String::new()
        } else {
            format!(" (${})", self.param.name)
        }
    }

    fn type_error(&self, given: &Value) -> NativeError {
        NativeError::type_error(format!(
            "{}(): Argument #{}{} must be {}, {} given",
            self.function,
            self.number,
            self.name(),
            expected(self.param),
            given_name(given)
        ))
    }

    /// Passing null where a scalar is expected still works in PHP 8, with a
    /// deprecation, but only for native functions and only without
    /// `strict_types`.
    fn null_deprecated(&self, context: &mut dyn Context) {
        context.deprecated(format!(
            "{}(): Passing null to parameter #{}{} of type {} is deprecated",
            self.function,
            self.number,
            self.name(),
            type_text(self.param.ty)
        ));
    }
}

/// The type as PHP writes it in messages.
fn type_text(ty: ParamType) -> &'static str {
    match ty {
        ParamType::Mixed => "mixed",
        ParamType::Bool => "bool",
        ParamType::Int => "int",
        ParamType::Float => "float",
        ParamType::String => "string",
        ParamType::Array => "array",
        ParamType::Object => "object",
        ParamType::Callable => "callable",
        ParamType::Iterable => "Traversable|array",
        ParamType::Number => "int|float",
        ParamType::IntOrString => "string|int",
        ParamType::ArrayOrString => "array|string",
    }
}

fn expected(param: &Param) -> String {
    let text = type_text(param.ty);
    match (param.nullable, text.contains('|')) {
        (false, _) => format!("of type {text}"),
        (true, false) => format!("of type ?{text}"),
        (true, true) => format!("of type {text}|null"),
    }
}

/// How PHP names the offending argument: `true` and `false` rather than
/// `bool`, and the class name for an object.
fn given_name(value: &Value) -> String {
    match value {
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        other => other.type_name(),
    }
}

fn convert(
    value: &mut Value,
    site: &Site<'_>,
    strict: bool,
    context: &mut dyn Context,
) -> Result<(), NativeError> {
    if value.is_null() && site.param.nullable {
        return Ok(());
    }
    let converted = match site.param.ty {
        // Whether a value is callable depends on which functions and classes
        // exist, which only the VM knows; `Context::call` reports it.
        ParamType::Mixed | ParamType::Callable => return Ok(()),
        ParamType::Array => matches!(value, Value::Array(_)).then_some(None),
        ParamType::Object => matches!(value, Value::Object(_)).then_some(None),
        ParamType::Iterable => matches!(value, Value::Array(_) | Value::Object(_)).then_some(None),
        ParamType::Bool => match value {
            Value::Bool(_) => Some(None),
            _ if strict => None,
            _ => to_bool(value, site, context).map(Some),
        },
        ParamType::Int => match value {
            Value::Int(_) => Some(None),
            _ if strict => None,
            _ => to_int(value, site, context).map(Some),
        },
        ParamType::Float => match value {
            Value::Float(_) => Some(None),
            // An int is accepted for a float even under strict_types.
            Value::Int(int) => Some(Some(Value::Float(*int as f64))),
            _ if strict => None,
            _ => to_float(value, site, context).map(Some),
        },
        ParamType::String => match value {
            Value::String(_) => Some(None),
            _ if strict => None,
            _ => to_string(value, site, context).map(Some),
        },
        ParamType::Number => match value {
            Value::Int(_) | Value::Float(_) => Some(None),
            _ if strict => None,
            _ => to_number(value, site, context).map(Some),
        },
        ParamType::IntOrString => match value {
            Value::Int(_) | Value::String(_) => Some(None),
            _ if strict => None,
            // A float that cannot be an int is kept as its string form.
            Value::Float(float) if float.is_nan() || !fits_int(*float) => {
                to_string(value, site, context).map(Some)
            }
            _ => to_int(value, site, context).map(Some),
        },
        ParamType::ArrayOrString => match value {
            Value::Array(_) | Value::String(_) => Some(None),
            _ if strict => None,
            _ => to_string(value, site, context).map(Some),
        },
    };
    match converted {
        Some(Some(new)) => *value = new,
        Some(None) => {}
        None => return Err(site.type_error(value)),
    }
    Ok(())
}

fn to_bool(value: &Value, site: &Site<'_>, context: &mut dyn Context) -> Option<Value> {
    match value {
        Value::Null => {
            site.null_deprecated(context);
            Some(Value::Bool(false))
        }
        Value::Int(_) | Value::Float(_) | Value::String(_) => {
            Some(Value::Bool(cast_to_bool(value, context.diagnostics())))
        }
        _ => None,
    }
}

/// `i64::MIN <= float < 2^63`, the range in which a float has an int value.
fn fits_int(float: f64) -> bool {
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    (-LIMIT..LIMIT).contains(&float)
}

/// A float is accepted for an int when it is in range; a fractional part is
/// dropped with a deprecation.
fn float_to_int(
    float: f64,
    lossy: impl FnOnce() -> String,
    context: &mut dyn Context,
) -> Option<i64> {
    if float.is_nan() || !fits_int(float) {
        return None;
    }
    let int = float as i64;
    if int as f64 != float {
        context.deprecated(lossy());
    }
    Some(int)
}

fn to_int(value: &Value, site: &Site<'_>, context: &mut dyn Context) -> Option<Value> {
    let int = match value {
        Value::Null => {
            site.null_deprecated(context);
            0
        }
        Value::Bool(flag) => i64::from(*flag),
        Value::Float(float) => float_to_int(
            *float,
            || {
                format!(
                    "Implicit conversion from float {} to int loses precision",
                    float_repr(*float)
                )
            },
            context,
        )?,
        Value::String(string) => match numeric(string)? {
            NumVal::Int(int) => int,
            NumVal::Float(float) => float_to_int(
                float,
                || {
                    format!(
                        "Implicit conversion from float-string \"{}\" to int loses precision",
                        string.to_string_lossy()
                    )
                },
                context,
            )?,
        },
        _ => return None,
    };
    Some(Value::Int(int))
}

fn to_float(value: &Value, site: &Site<'_>, context: &mut dyn Context) -> Option<Value> {
    let float = match value {
        Value::Null => {
            site.null_deprecated(context);
            0.0
        }
        Value::Bool(flag) => f64::from(u8::from(*flag)),
        Value::String(string) => match numeric(string)? {
            NumVal::Int(int) => int as f64,
            NumVal::Float(float) => float,
        },
        _ => return None,
    };
    Some(Value::Float(float))
}

fn to_number(value: &Value, site: &Site<'_>, context: &mut dyn Context) -> Option<Value> {
    Some(match value {
        Value::Null => {
            site.null_deprecated(context);
            Value::Int(0)
        }
        Value::Bool(flag) => Value::Int(i64::from(*flag)),
        Value::String(string) => match numeric(string)? {
            NumVal::Int(int) => Value::Int(int),
            NumVal::Float(float) => Value::Float(float),
        },
        _ => return None,
    })
}

/// Stringable objects are converted by the VM before entering the native parser.
fn to_string(value: &Value, site: &Site<'_>, context: &mut dyn Context) -> Option<Value> {
    match value {
        Value::Null => {
            site.null_deprecated(context);
            Some(Value::String(PhpString::new()))
        }
        Value::Bool(_) | Value::Int(_) | Value::Float(_) => {
            to_php_string(value, context.diagnostics())
                .ok()
                .map(Value::String)
        }
        _ => None,
    }
}

/// The number a string stands for, if the whole string is numeric. Unlike in
/// arithmetic, `"12abc"` is not accepted for a parameter.
fn numeric(string: &PhpString) -> Option<NumVal> {
    let parsed = parse_numeric(string.as_bytes());
    if parsed.exact { parsed.value } else { None }
}

fn check_count(name: &str, params: &[Param], given: usize) -> Result<(), NativeError> {
    let min = params.iter().filter(|param| !param.optional).count();
    let max = match params.last() {
        Some(last) if last.variadic => None,
        _ => Some(params.len()),
    };
    let (bound, expected) = match max {
        Some(max) if given > max && min == max => ("exactly", max),
        Some(max) if given > max => ("at most", max),
        Some(max) if given < min && min == max => ("exactly", min),
        _ if given < min => ("at least", min),
        _ => return Ok(()),
    };
    let plural = if expected == 1 { "" } else { "s" };
    Err(NativeError::argument_count_error(format!(
        "{name}() expects {bound} {expected} argument{plural}, {given} given"
    )))
}

/// Typed access to prepared arguments.
///
/// Every helper returns `None` when the argument was left out or is null, so
/// a required parameter reads as `args::int(arguments, 0).unwrap_or_default()`
/// and an optional one as `args::int(arguments, 1).unwrap_or(10)`.
pub mod args {
    use std::cell::RefCell;
    use std::rc::Rc;

    use phera_value::{Object, PhpArray, PhpString, Value};

    pub fn bool(arguments: &[Value], index: usize) -> Option<bool> {
        match arguments.get(index)? {
            Value::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn int(arguments: &[Value], index: usize) -> Option<i64> {
        match arguments.get(index)? {
            Value::Int(value) => Some(*value),
            _ => None,
        }
    }

    /// Reads a `Float` parameter, or the float value of a `Number` one.
    pub fn float(arguments: &[Value], index: usize) -> Option<f64> {
        match arguments.get(index)? {
            Value::Float(value) => Some(*value),
            Value::Int(value) => Some(*value as f64),
            _ => None,
        }
    }

    pub fn string(arguments: &[Value], index: usize) -> Option<&PhpString> {
        match arguments.get(index)? {
            Value::String(value) => Some(value),
            _ => None,
        }
    }

    /// The bytes of a string argument; empty when it was left out or is null.
    pub fn bytes(arguments: &[Value], index: usize) -> &[u8] {
        string(arguments, index).map_or(&[], PhpString::as_bytes)
    }

    pub fn array(arguments: &[Value], index: usize) -> Option<&Rc<PhpArray>> {
        match arguments.get(index)? {
            Value::Array(value) => Some(value),
            _ => None,
        }
    }

    pub fn object(arguments: &[Value], index: usize) -> Option<&Rc<Object>> {
        match arguments.get(index)? {
            Value::Object(value) => Some(value),
            _ => None,
        }
    }

    /// The caller's variable behind a by-reference parameter.
    pub fn reference(arguments: &[Value], index: usize) -> Option<&Rc<RefCell<Value>>> {
        match arguments.get(index)? {
            Value::Reference(value) => Some(value),
            _ => None,
        }
    }

    /// The argument as it is; null when it was left out.
    pub fn value(arguments: &[Value], index: usize) -> &Value {
        arguments.get(index).unwrap_or(&Value::Null)
    }

    /// Whether the caller passed this argument at all (null counts as passed).
    pub fn given(arguments: &[Value], index: usize) -> bool {
        index < arguments.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestContext;
    use crate::{NativeResult, Param, ParamType};

    fn nothing(_context: &mut dyn Context, _arguments: &[Value]) -> NativeResult {
        Ok(Value::Null)
    }

    fn count_error(params: &'static [Param], given: usize) -> Option<String> {
        let entry = FunctionEntry::sync("f", params, nothing);
        let mut arguments = vec![Value::Null; given];
        match prepare_arguments(&entry, &mut arguments, &mut TestContext::default()) {
            Ok(()) => None,
            Err(NativeError::Throw { class, message, .. }) => Some(format!("{class}: {message}")),
            Err(NativeError::Unwind(_)) => Some("unwind".to_string()),
        }
    }

    #[test]
    fn argument_count_messages_match_php() {
        const ONE: &[Param] = &[Param::new("a", ParamType::Mixed)];
        const RANGE: &[Param] = &[
            Param::new("a", ParamType::Mixed),
            Param::new("b", ParamType::Mixed),
            Param::new("c", ParamType::Mixed),
            Param::new("d", ParamType::Mixed).optional(),
        ];
        const REST: &[Param] = &[
            Param::new("a", ParamType::Mixed),
            Param::new("rest", ParamType::Mixed).variadic(),
        ];

        assert_eq!(count_error(ONE, 1), None);
        assert_eq!(
            count_error(ONE, 0).as_deref(),
            Some("ArgumentCountError: f() expects exactly 1 argument, 0 given")
        );
        assert_eq!(
            count_error(ONE, 2).as_deref(),
            Some("ArgumentCountError: f() expects exactly 1 argument, 2 given")
        );
        assert_eq!(
            count_error(RANGE, 2).as_deref(),
            Some("ArgumentCountError: f() expects at least 3 arguments, 2 given")
        );
        assert_eq!(
            count_error(RANGE, 5).as_deref(),
            Some("ArgumentCountError: f() expects at most 4 arguments, 5 given")
        );
        assert_eq!(count_error(RANGE, 4), None);
        assert_eq!(
            count_error(REST, 0).as_deref(),
            Some("ArgumentCountError: f() expects at least 1 argument, 0 given")
        );
        assert_eq!(count_error(REST, 9), None);
    }

    fn prepare(params: &'static [Param], arguments: &mut [Value]) -> Result<(), String> {
        let entry = FunctionEntry::sync("f", params, nothing);
        match prepare_arguments(&entry, arguments, &mut TestContext::default()) {
            Ok(()) => Ok(()),
            Err(NativeError::Throw { message, .. }) => Err(message),
            Err(NativeError::Unwind(_)) => Err("unwind".to_string()),
        }
    }

    #[test]
    fn nullable_parameters_keep_null_and_name_it_in_errors() {
        const PARAMS: &[Param] = &[
            Param::new("length", ParamType::Int).nullable(),
            Param::new("key", ParamType::IntOrString).nullable(),
        ];
        let mut arguments = [Value::Null, Value::Null];
        assert_eq!(prepare(PARAMS, &mut arguments), Ok(()));
        assert!(arguments.iter().all(Value::is_null));

        let mut arguments = [Value::from("x"), Value::Null];
        assert_eq!(
            prepare(PARAMS, &mut arguments).unwrap_err(),
            "f(): Argument #1 ($length) must be of type ?int, string given"
        );
        let empty = Value::from(phera_value::PhpArray::new());
        let mut arguments = [Value::Int(1), empty];
        assert_eq!(
            prepare(PARAMS, &mut arguments).unwrap_err(),
            "f(): Argument #2 ($key) must be of type string|int|null, array given"
        );
    }

    #[test]
    fn by_reference_parameters_are_checked_inside_the_reference() {
        use std::cell::RefCell;
        use std::rc::Rc;

        const PARAMS: &[Param] = &[
            Param::new("array", ParamType::Array).by_ref(),
            Param::new("count", ParamType::Mixed).by_ref().optional(),
        ];
        let variable = Rc::new(RefCell::new(Value::Null));
        let mut arguments = [Value::Reference(variable.clone())];
        assert_eq!(
            prepare(PARAMS, &mut arguments).unwrap_err(),
            "f(): Argument #1 ($array) must be of type array, null given"
        );

        *variable.borrow_mut() = Value::from(phera_value::PhpArray::new());
        let untouched = Rc::new(RefCell::new(Value::from("anything")));
        let mut arguments = [
            Value::Reference(variable),
            Value::Reference(untouched.clone()),
        ];
        assert_eq!(prepare(PARAMS, &mut arguments), Ok(()));
        assert!(arguments.iter().all(Value::is_reference));
        assert_eq!(*untouched.borrow(), Value::from("anything"));
    }

    #[test]
    fn a_reference_passed_by_value_is_replaced_by_its_value() {
        use std::cell::RefCell;
        use std::rc::Rc;

        const PARAMS: &[Param] = &[Param::new("value", ParamType::Mixed)];
        let mut arguments = [Value::Reference(Rc::new(RefCell::new(Value::Int(7))))];
        assert_eq!(prepare(PARAMS, &mut arguments), Ok(()));
        assert_eq!(arguments[0], Value::Int(7));
    }
}
