//! `ini_set('session.use_strict_mode', '1')` changes only the current request.

use phera_extension_api::value::{NoDiagnostics, is_numeric, to_php_string};
use phera_extension_api::{Context, NativeError, NativeResult, PhpString, Value};

use crate::{
    id::c_string,
    quantity,
    state::{SessionState, can_change, state, warning},
};

#[derive(Clone, Copy)]
enum ChangeWhen {
    BeforeStartAndOutput,
    Never,
}

// The request settings registry can consume these names, defaults and mutation guards.
const SETTINGS: &[(&str, &str, ChangeWhen)] = &{
    use ChangeWhen::{BeforeStartAndOutput, Never};
    [
        ("name", "PHPSESSID", BeforeStartAndOutput),
        ("save_path", "", BeforeStartAndOutput),
        ("save_handler", "files", BeforeStartAndOutput),
        ("use_cookies", "1", BeforeStartAndOutput),
        ("use_only_cookies", "1", BeforeStartAndOutput),
        ("use_strict_mode", "0", BeforeStartAndOutput),
        ("cookie_lifetime", "0", BeforeStartAndOutput),
        ("cookie_path", "/", BeforeStartAndOutput),
        ("cookie_domain", "", BeforeStartAndOutput),
        ("cookie_secure", "0", BeforeStartAndOutput),
        ("cookie_partitioned", "0", BeforeStartAndOutput),
        ("cookie_httponly", "0", BeforeStartAndOutput),
        ("cookie_samesite", "", BeforeStartAndOutput),
        ("gc_probability", "1", BeforeStartAndOutput),
        ("gc_divisor", "100", BeforeStartAndOutput),
        ("gc_maxlifetime", "1440", BeforeStartAndOutput),
        ("serialize_handler", "php", BeforeStartAndOutput),
        ("cache_limiter", "nocache", BeforeStartAndOutput),
        ("cache_expire", "180", BeforeStartAndOutput),
        ("lazy_write", "1", BeforeStartAndOutput),
        ("sid_length", "32", BeforeStartAndOutput),
        ("sid_bits_per_character", "4", BeforeStartAndOutput),
        ("auto_start", "0", Never),
        ("use_trans_sid", "0", BeforeStartAndOutput),
    ]
};

pub fn ini(context: &mut dyn Context, name: &[u8], value: Option<&Value>) -> NativeResult {
    let Some(name) = name
        .strip_prefix(b"session.")
        .and_then(|name| std::str::from_utf8(name).ok())
    else {
        return Ok(Value::Bool(false));
    };
    let value = value
        .map(|value| {
            let value = value.try_deref()?;
            if matches!(
                &*value,
                Value::Array(_) | Value::Object(_) | Value::Resource(_)
            ) {
                return Err(NativeError::type_error(
                    "ini_set(): Argument #2 ($value) must be of type string|int|float|bool|null",
                ));
            }
            Ok(to_php_string(&value, &mut NoDiagnostics)?)
        })
        .transpose()?;
    let Some(previous) = get(context, name) else {
        if value.is_some() && name.starts_with("trans_sid_") {
            return Err(unsupported(name));
        }
        return Ok(Value::Bool(false));
    };
    if let Some(value) = value
        && !update(context, name, value, "ini_set")?
    {
        return Ok(Value::Bool(false));
    }
    Ok(Value::String(previous))
}

fn definition(setting: &str) -> Option<(&'static str, &'static str, ChangeWhen)> {
    SETTINGS
        .iter()
        .find(|(name, _, _)| *name == setting)
        .copied()
}

pub fn get(context: &mut dyn Context, setting: &str) -> Option<PhpString> {
    let (name, initial, _) = definition(setting)?;
    Some(
        context
            .state()
            .get::<SessionState>()
            .and_then(|session| session.settings.get(name))
            .cloned()
            .unwrap_or_else(|| initial.into()),
    )
}

pub fn text(context: &mut dyn Context, setting: &str) -> PhpString {
    get(context, setting).unwrap_or_default()
}

pub fn integer(context: &mut dyn Context, setting: &str) -> i64 {
    if let Some(number) = context
        .state()
        .get::<SessionState>()
        .and_then(|session| session.numbers.get(setting))
    {
        return *number;
    }
    definition(setting)
        .and_then(|(_, initial, _)| initial.parse().ok())
        .unwrap_or_default()
}

pub fn boolean(context: &mut dyn Context, setting: &str) -> bool {
    ini_bool(&text(context, setting))
}

fn ini_bool(value: &PhpString) -> bool {
    let bytes = value.as_bytes();
    [b"on".as_slice(), b"yes", b"true"]
        .iter()
        .any(|word| bytes.eq_ignore_ascii_case(word))
        || number(value) as i32 != 0
}

fn number(value: &PhpString) -> i64 {
    let value = c_string(value.as_bytes()).trim_ascii_start();
    let negative = value.first() == Some(&b'-');
    let value = value
        .strip_prefix(b"-")
        .or_else(|| value.strip_prefix(b"+"))
        .unwrap_or(value);
    let digits = value.iter().take_while(|byte| byte.is_ascii_digit());
    let mut number = 0i64;
    for digit in digits {
        let digit = i64::from(digit - b'0');
        let digit = if negative { -digit } else { digit };
        number = number.saturating_mul(10).saturating_add(digit);
    }
    number
}

pub fn update(
    context: &mut dyn Context,
    setting: &str,
    value: PhpString,
    function: &str,
) -> Result<bool, NativeError> {
    let Some((name, initial, change)) = definition(setting) else {
        if setting.starts_with("trans_sid_") {
            return Err(unsupported(setting));
        }
        return Ok(false);
    };
    let changeable = match change {
        ChangeWhen::BeforeStartAndOutput => can_change(context, function, "ini settings", true),
        ChangeWhen::Never => false,
    };
    if !changeable {
        return Ok(false);
    }
    if matches!(
        name,
        "cookie_lifetime" | "gc_probability" | "gc_divisor" | "gc_maxlifetime" | "cache_expire"
    ) {
        return Ok(update_quantity(context, name, value, function));
    }
    let bytes = c_string(value.as_bytes());
    let printable = String::from_utf8_lossy(bytes);
    let problem = match name {
        "sid_length" | "sid_bits_per_character" if bytes != initial.as_bytes() => {
            return Err(unsupported(name));
        }
        "use_trans_sid" if ini_bool(&value) => return Err(unsupported(name)),
        "use_only_cookies" if !ini_bool(&value) => {
            context.deprecated(format!(
                "{function}(): Disabling \
                session.use_only_cookies INI setting is deprecated"
            ));
            None
        }
        "save_path" if value.as_bytes().contains(&0) => return Ok(false),
        "name"
            if bytes.is_empty()
                || bytes != value.as_bytes()
                || is_numeric(value.as_bytes())
                || bytes
                    .iter()
                    .any(|byte| b"=,;.[ \t\r\n\x0b\x0c".contains(byte)) =>
        {
            Some(format!(
                "session.name \"{printable}\" must not be numeric, empty, contain null \
                 bytes or any of the following characters \"=,;.[ \\t\\r\\n\\013\\014\""
            ))
        }
        "save_handler" if bytes.eq_ignore_ascii_case(b"user") => {
            Some("Session save handler \"user\" cannot be set by ini_set()".into())
        }
        "save_handler" if !bytes.eq_ignore_ascii_case(b"files") => Some(format!(
            "Session save handler \"{printable}\" cannot be found"
        )),
        "serialize_handler"
            if ![b"php".as_slice(), b"php_serialize", b"php_binary"]
                .iter()
                .any(|format| bytes.eq_ignore_ascii_case(format)) =>
        {
            Some(format!(
                "Serialization handler \"{printable}\" cannot be found"
            ))
        }
        _ => None,
    };
    if let Some(message) = problem {
        warning(context, function, &message);
        return Ok(false);
    }
    state(context).settings.insert(name, value);
    Ok(true)
}

fn update_quantity(
    context: &mut dyn Context,
    name: &'static str,
    value: PhpString,
    function: &str,
) -> bool {
    if name == "cookie_lifetime" {
        let decimal = number(&value);
        if decimal < 0 {
            warning(context, function, "CookieLifetime cannot be negative");
            return false;
        }
        if decimal > i64::MAX - i64::from(i32::MAX) - 1 {
            state(context).settings.insert(name, value);
            return true;
        }
    }
    let (number, diagnostic) = quantity::parse(value.as_bytes());
    if let Some(diagnostic) = diagnostic {
        context.warning(format!("Invalid \"session.{name}\" setting. {diagnostic}"));
    }
    let problem = match name {
        "gc_probability" if number < 0 => Some("must be greater than or equal to 0"),
        "gc_divisor" if number <= 0 => Some("must be greater than 0"),
        "cookie_lifetime" if number < 0 => return false,
        _ => None,
    };
    if let Some(problem) = problem {
        warning(context, function, &format!("session.{name} {problem}"));
        return false;
    }
    let session = state(context);
    session.settings.insert(name, value);
    session.numbers.insert(name, number);
    true
}

fn unsupported(setting: &str) -> NativeError {
    NativeError::throw(
        "Phera\\NotSupportedException",
        format!("session.{setting} is not supported by Phera: deprecated since PHP 8.4"),
    )
}
