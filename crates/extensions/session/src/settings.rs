//! `ini_set('session.use_strict_mode', '1')` changes only the current request.

use phera_extension_api::value::{NoDiagnostics, is_numeric, to_php_string};
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpString, Value, args,
};

use crate::{
    id::c_string,
    state::{active, can_change, state},
};

pub const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::sync("session_name", NAME, |context, arguments| {
        string_setting(context, arguments, "session_name", "name", "name", true)
    }),
    FunctionEntry::sync("session_save_path", PATH, |context, arguments| {
        string_setting(
            context,
            arguments,
            "session_save_path",
            "save_path",
            "save path",
            true,
        )
    }),
    FunctionEntry::sync("session_cache_limiter", VALUE, |context, arguments| {
        string_setting(
            context,
            arguments,
            "session_cache_limiter",
            "cache_limiter",
            "cache limiter",
            false,
        )
    }),
    FunctionEntry::sync("session_cache_expire", EXPIRE, cache_expire),
    FunctionEntry::sync("session_module_name", MODULE, module_name),
];

const NAME: &[Param] = &[Param::new("name", ParamType::String).nullable().optional()];
const PATH: &[Param] = &[Param::new("path", ParamType::String).nullable().optional()];
const VALUE: &[Param] = &[Param::new("value", ParamType::String).nullable().optional()];
const EXPIRE: &[Param] = &[Param::new("value", ParamType::Int).nullable().optional()];
const MODULE: &[Param] = &[Param::new("module", ParamType::String)
    .nullable()
    .optional()];

const DEFAULTS: &[(&str, &str)] = &[
    ("name", "PHPSESSID"),
    ("save_path", ""),
    ("save_handler", "files"),
    ("use_cookies", "1"),
    ("use_only_cookies", "1"),
    ("use_strict_mode", "0"),
    ("cookie_lifetime", "0"),
    ("cookie_path", "/"),
    ("cookie_domain", ""),
    ("cookie_secure", "0"),
    ("cookie_partitioned", "0"),
    ("cookie_httponly", "0"),
    ("cookie_samesite", ""),
    ("gc_probability", "1"),
    ("gc_divisor", "100"),
    ("gc_maxlifetime", "1440"),
    ("serialize_handler", "php"),
    ("cache_limiter", "nocache"),
    ("cache_expire", "180"),
    ("lazy_write", "1"),
    ("sid_length", "32"),
    ("sid_bits_per_character", "4"),
    ("auto_start", "0"),
    ("use_trans_sid", "0"),
];

pub fn ini(context: &mut dyn Context, name: &[u8], value: Option<&Value>) -> NativeResult {
    let Some(name) = name
        .strip_prefix(b"session.")
        .and_then(|name| std::str::from_utf8(name).ok())
    else {
        return Ok(Value::Bool(false));
    };
    let Some(previous) = get(context, name) else {
        if value.is_some() && name.starts_with("trans_sid_") {
            return Err(unsupported(name));
        }
        return Ok(Value::Bool(false));
    };
    if let Some(value) = value {
        let value = to_php_string(value, &mut NoDiagnostics)?;
        if !update(context, name, value, "ini_set")? {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::String(previous))
}

fn default(setting: &str) -> Option<(&'static str, &'static str)> {
    DEFAULTS.iter().find(|(name, _)| *name == setting).copied()
}

pub fn get(context: &mut dyn Context, setting: &str) -> Option<PhpString> {
    let (name, initial) = default(setting)?;
    Some(
        state(context)
            .settings
            .get(name)
            .cloned()
            .unwrap_or_else(|| initial.into()),
    )
}

pub fn text(context: &mut dyn Context, setting: &str) -> PhpString {
    get(context, setting).unwrap_or_default()
}

pub fn integer(context: &mut dyn Context, setting: &str) -> i64 {
    number(&text(context, setting))
}

pub fn boolean(context: &mut dyn Context, setting: &str) -> bool {
    ini_bool(&text(context, setting))
}

fn ini_bool(value: &PhpString) -> bool {
    let bytes = c_string(value.as_bytes());
    [b"on".as_slice(), b"yes", b"true"]
        .iter()
        .any(|word| bytes.eq_ignore_ascii_case(word))
        || number(value) != 0
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
    let Some((name, initial)) = default(setting) else {
        if setting.starts_with("trans_sid_") {
            return Err(unsupported(setting));
        }
        return Ok(false);
    };
    if name == "auto_start" || !can_change(context, function, "ini settings", true) {
        return Ok(false);
    }
    let bytes = c_string(value.as_bytes());
    let printable = String::from_utf8_lossy(bytes);
    let warning = match name {
        "sid_length" | "sid_bits_per_character" if bytes != initial.as_bytes() => {
            return Err(unsupported(name));
        }
        "use_trans_sid" if ini_bool(&value) => return Err(unsupported(name)),
        "use_only_cookies" if !ini_bool(&value) => {
            context.deprecated(format!(
                "{function}(session.configuration): Disabling \
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
                "session.name \"{printable}\" must not be numeric, empty, contain null bytes or any of \
             the following characters \"=,;.[ \\t\\r\\n\\013\\014\""
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
        "cookie_lifetime" if number(&value) < 0 => Some("CookieLifetime cannot be negative".into()),
        "gc_probability" if number(&value) < 0 => {
            context.warning(format!(
                "{function}(session.gc_probability): \
                session.gc_probability must be greater than or equal to 0"
            ));
            return Ok(false);
        }
        "gc_divisor" if number(&value) <= 0 => {
            context.warning(format!(
                "{function}(session.gc_divisor): \
                session.gc_divisor must be greater than 0"
            ));
            return Ok(false);
        }
        _ => None,
    };
    if let Some(message) = warning {
        context.warning(format!("{function}(): {message}"));
        return Ok(false);
    }
    state(context).settings.insert(name, value);
    Ok(true)
}

fn unsupported(setting: &str) -> NativeError {
    NativeError::throw(
        "Phera\\NotSupportedException",
        format!("session.{setting} is not supported by Phera: deprecated since PHP 8.4"),
    )
}

fn string_setting(
    context: &mut dyn Context,
    arguments: &[Value],
    function: &str,
    setting: &str,
    label: &str,
    path: bool,
) -> NativeResult {
    let previous = text(context, setting);
    if let Some(next) = args::string(arguments, 0) {
        if path && next.as_bytes().contains(&0) {
            let parameter = if setting == "name" { "name" } else { "path" };
            return Err(NativeError::value_error(format!(
                "{function}(): Argument #1 (${parameter}) must not contain any null bytes"
            )));
        }
        if !can_change(context, function, label, true) {
            return Ok(Value::Bool(false));
        }
        update(context, setting, next.clone(), function)?;
    }
    Ok(Value::String(previous))
}

fn cache_expire(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let previous = Value::Int(integer(context, "cache_expire"));
    if let Some(next) = args::int(arguments, 0) {
        if !can_change(context, "session_cache_expire", "cache expiration", true) {
            return Ok(if active(context) {
                previous
            } else {
                Value::Bool(false)
            });
        }
        update(
            context,
            "cache_expire",
            next.to_string().into(),
            "session_cache_expire",
        )?;
    }
    Ok(previous)
}

fn module_name(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let current = text(context, "save_handler");
    let previous = Value::from(if current.as_bytes() == b"user" {
        "user"
    } else {
        "files"
    });
    if let Some(next) = args::string(arguments, 0) {
        let function = "session_module_name";
        if !can_change(context, function, "save handler module", true) {
            return Ok(Value::Bool(false));
        }
        if next.as_bytes().eq_ignore_ascii_case(b"user") {
            return Err(NativeError::value_error(
                "session_module_name(): Argument #1 ($module) cannot be \"user\"",
            ));
        }
        let name = c_string(next.as_bytes());
        if !name.eq_ignore_ascii_case(b"files") {
            let name = String::from_utf8_lossy(name);
            context.warning(format!(
                "{function}(): Session handler module \"{name}\" cannot be found"
            ));
            return Ok(Value::Bool(false));
        }
        update(context, "save_handler", next.clone(), function)?;
    }
    Ok(previous)
}
