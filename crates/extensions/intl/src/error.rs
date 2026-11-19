//! Intl's last error belongs to the current PHP request.
//!
//! ```php
//! normalizer_normalize("\xff"); // false
//! intl_get_error_code();         // U_INVALID_CHAR_FOUND
//! intl_is_failure(U_USING_FALLBACK_WARNING); // false
//! ```

use std::collections::HashMap;
use std::sync::OnceLock;

use phera_extension_api::{Context, FunctionEntry, NativeError, NativeResult, Param, ParamType};
use phera_extension_api::{Registry, Value, args};

pub const ILLEGAL_ARGUMENT: i32 = 1;
pub const INVALID_CHAR: i32 = 10;

#[derive(Default)]
pub struct ErrorState {
    code: i32,
    detail: Option<String>,
}

impl ErrorState {
    fn message(&self) -> String {
        let name = name(self.code);
        match &self.detail {
            Some(detail) => format!("{detail}: {name}"),
            None => name.to_owned(),
        }
    }
}

pub fn clear(context: &mut dyn Context) {
    *context.state().get_or_default::<ErrorState>() = ErrorState::default();
}

pub fn fail(
    context: &mut dyn Context,
    function: &str,
    code: i32,
    detail: &str,
    result: Value,
) -> NativeResult {
    let detail = format!("{function}(): {detail}");
    *context.state().get_or_default::<ErrorState>() = ErrorState {
        code,
        detail: Some(detail.clone()),
    };
    let level = context.ini_get(b"intl.error_level").map_or(0, |value| {
        phera_extension_api::ini::integer(value.as_bytes())
    });
    match level {
        2 | 512 => context.warning(detail.clone()),
        8 | 1024 => context.notice(detail.clone()),
        8192 | 16384 => context.deprecated(detail.clone()),
        _ => {}
    }
    if context
        .ini_get(b"intl.use_exceptions")
        .is_some_and(|value| phera_extension_api::ini::boolean(value.as_bytes()))
    {
        return Err(NativeError::throw("IntlException", detail));
    }
    Ok(result)
}

pub fn name(code: i32) -> &'static str {
    static NAMES: OnceLock<HashMap<i32, &str>> = OnceLock::new();
    NAMES
        .get_or_init(|| {
            include_str!("../data/icu-error-names.txt")
                .lines()
                .filter_map(|row| {
                    let (number, name) = row.split_once(' ')?;
                    Some((number.parse().ok()?, name))
                })
                .collect()
        })
        .get(&code)
        .copied()
        .unwrap_or("[BOGUS UErrorCode]")
}

pub fn register(registry: &mut Registry) {
    for row in include_str!("../data/constants.txt").lines() {
        if let Some((name, number)) = row.split_once(' ')
            && let Ok(number) = number.parse::<i64>()
        {
            registry.constant(name, number);
        }
    }
    registry.constant("INTL_ICU_VERSION", "2.4.0");
    registry.constant("INTL_ICU_DATA_VERSION", "49");

    const CODE: &[Param] = &[Param::new("errorCode", ParamType::Int)];
    for (entry, result) in [
        (
            FunctionEntry::sync("intl_get_error_code", &[], get_code),
            "int",
        ),
        (
            FunctionEntry::sync("intl_get_error_message", &[], get_message),
            "string",
        ),
        (
            FunctionEntry::sync("intl_is_failure", CODE, is_failure),
            "bool",
        ),
        (
            FunctionEntry::sync("intl_error_name", CODE, error_name),
            "string",
        ),
    ] {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
}

fn get_code(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    let code = context.state().get_or_default::<ErrorState>().code;
    Ok(Value::Int(i64::from(code)))
}

fn get_message(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::from(
        context.state().get_or_default::<ErrorState>().message(),
    ))
}

fn is_failure(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(args::int(arguments, 0).unwrap_or(0) > 0))
}

fn error_name(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let code = args::int(arguments, 0).unwrap_or(0) as i32;
    Ok(Value::from(name(code)))
}
