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

use phera_value::Value;

use crate::{Context, FunctionEntry, NativeError};

/// Validates `arguments` against the signature of `entry` and converts scalar
/// arguments in place. On failure the error is the exception PHP would throw
/// (`ArgumentCountError` or `TypeError`), with PHP's message.
pub fn prepare_arguments(
    entry: &FunctionEntry,
    arguments: &mut [Value],
    context: &mut dyn Context,
) -> Result<(), NativeError> {
    check_count(entry, arguments.len())?;
    // Type checks and scalar conversion are added next; see the module docs
    // for the guarantees they establish.
    let _ = context;
    Ok(())
}

fn check_count(entry: &FunctionEntry, given: usize) -> Result<(), NativeError> {
    let min = entry.min_args();
    let max = entry.max_args();
    let (bound, expected) = match max {
        Some(max) if given > max && min == max => ("exactly", max),
        Some(max) if given > max => ("at most", max),
        Some(max) if given < min && min == max => ("exactly", min),
        _ if given < min => ("at least", min),
        _ => return Ok(()),
    };
    let plural = if expected == 1 { "" } else { "s" };
    Err(NativeError::argument_count_error(format!(
        "{}() expects {bound} {expected} argument{plural}, {given} given",
        entry.name
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
            Err(NativeError::Throw { class, message }) => Some(format!("{class}: {message}")),
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
}
