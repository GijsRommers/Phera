//! `ini_set('session.use_strict_mode', '1')` changes only the current request.

use phera_extension_api::value::is_numeric;
use phera_extension_api::{Context, NativeError, PhpString, Registry, ini};

use crate::{
    id::c_string,
    quantity,
    state::{SessionState, can_change, state, warning},
};

const DIRECTIVES: &[&str] = &[
    "session.name",
    "session.save_path",
    "session.save_handler",
    "session.use_cookies",
    "session.use_only_cookies",
    "session.use_strict_mode",
    "session.cookie_lifetime",
    "session.cookie_path",
    "session.cookie_domain",
    "session.cookie_secure",
    "session.cookie_partitioned",
    "session.cookie_httponly",
    "session.cookie_samesite",
    "session.gc_probability",
    "session.gc_divisor",
    "session.gc_maxlifetime",
    "session.serialize_handler",
    "session.cache_limiter",
    "session.cache_expire",
    "session.lazy_write",
    "session.sid_length",
    "session.sid_bits_per_character",
    "session.use_trans_sid",
    "session.trans_sid_hosts",
    "session.trans_sid_tags",
];

pub(crate) fn register(registry: &mut Registry) {
    for name in DIRECTIVES {
        registry.ini_update(name, apply);
    }
}

pub fn text(context: &dyn Context, setting: &str) -> PhpString {
    context
        .ini_get(format!("session.{setting}").as_bytes())
        .unwrap_or_default()
}

pub fn integer(context: &mut dyn Context, setting: &str) -> i64 {
    if let Some(number) = context
        .state()
        .get::<SessionState>()
        .and_then(|session| session.numbers.get(setting))
    {
        return *number;
    }
    ini::integer(text(context, setting).as_bytes())
}

pub fn boolean(context: &dyn Context, setting: &str) -> bool {
    ini::boolean(text(context, setting).as_bytes())
}

pub fn update(
    context: &mut dyn Context,
    setting: &str,
    value: PhpString,
    function: &str,
) -> Result<bool, NativeError> {
    Ok(context
        .ini_set(function, format!("session.{setting}").as_bytes(), value)?
        .is_some())
}

fn apply(
    context: &mut dyn Context,
    directive: &str,
    value: Option<&[u8]>,
    function: &str,
) -> Result<bool, NativeError> {
    let Some(name) = DIRECTIVES
        .iter()
        .find(|&&name| name == directive)
        .and_then(|name| name.strip_prefix("session."))
    else {
        return Ok(false);
    };
    if name.starts_with("trans_sid_") {
        return Err(unsupported(name));
    }
    if !can_change(context, function, "ini settings", true) {
        return Ok(false);
    }
    let value = value.unwrap_or_default();
    if matches!(
        name,
        "cookie_lifetime" | "gc_probability" | "gc_divisor" | "gc_maxlifetime" | "cache_expire"
    ) {
        return Ok(update_quantity(context, name, value, function));
    }
    let bytes = c_string(value);
    let printable = String::from_utf8_lossy(bytes);
    let problem = match name {
        "sid_length" if bytes != b"32" => {
            return Err(unsupported(name));
        }
        "sid_bits_per_character" if bytes != b"4" => return Err(unsupported(name)),
        "use_trans_sid" if ini::boolean(value) => return Err(unsupported(name)),
        "use_only_cookies" if !ini::boolean(value) => {
            context.deprecated(format!(
                "{function}(): Disabling \
                session.use_only_cookies INI setting is deprecated"
            ));
            None
        }
        "save_path" if value.contains(&0) => return Ok(false),
        "name"
            if bytes.is_empty()
                || bytes != value
                || is_numeric(value)
                || bytes
                    .iter()
                    .any(|byte| b"=,;.[ \t\r\n\x0b\x0c".contains(byte)) =>
        {
            Some(format!(
                "session.name \"{printable}\" must not be numeric, empty, contain null \
                 bytes or any of the following characters \"=,;.[ \\t\\r\\n\\013\\014\""
            ))
        }
        "save_handler" if function == "session_set_save_handler" => None,
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
    Ok(true)
}

fn update_quantity(
    context: &mut dyn Context,
    name: &'static str,
    value: &[u8],
    function: &str,
) -> bool {
    if name == "cookie_lifetime" {
        let decimal = ini::integer(c_string(value));
        if decimal < 0 {
            warning(context, function, "CookieLifetime cannot be negative");
            return false;
        }
        if decimal > i64::MAX - i64::from(i32::MAX) - 1 {
            return true;
        }
    }
    let (number, diagnostic) = quantity::parse(value);
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
    state(context).numbers.insert(name, number);
    true
}

fn unsupported(setting: &str) -> NativeError {
    NativeError::throw(
        "Phera\\NotSupportedException",
        format!("session.{setting} is not supported by Phera: deprecated since PHP 8.4"),
    )
}
