//! Cookies are appended to the response in call order.
//!
//! `setcookie('theme', 'dark mode', ['path' => '/', 'httponly' => true])`
//! adds `Set-Cookie: theme=dark%20mode; path=/; HttpOnly`.

use std::time::{SystemTime, UNIX_EPOCH};

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value,
    args,
    value::{to_bool, to_int, to_php_string},
};

use crate::{ResponseHead, headers_open, valid_header_line};

const PARAMS: &[Param] = &[
    Param::new("name", ParamType::String),
    Param::new("value", ParamType::String).optional(),
    Param::new("expires_or_options", ParamType::ArrayOrInt).optional(),
    Param::new("path", ParamType::String).optional(),
    Param::new("domain", ParamType::String).optional(),
    Param::new("secure", ParamType::Bool).optional(),
    Param::new("httponly", ParamType::Bool).optional(),
];

const INVALID: &str = r#"",", ";", " ", "\t", "\r", "\n", "\013", or "\014""#;

#[derive(Clone, Copy)]
enum Cookie {
    Encoded,
    Raw,
}

impl Cookie {
    fn name(self) -> &'static str {
        match self {
            Self::Encoded => "setcookie",
            Self::Raw => "setrawcookie",
        }
    }
}

pub(super) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "setcookie",
        PARAMS,
        |context, arguments| set_cookie(context, arguments, Cookie::Encoded),
    ));
    registry.function(FunctionEntry::sync(
        "setrawcookie",
        PARAMS,
        |context, arguments| set_cookie(context, arguments, Cookie::Raw),
    ));
}

#[derive(Default)]
struct Options {
    expires: i64,
    path: Vec<u8>,
    domain: Vec<u8>,
    secure: bool,
    httponly: bool,
    samesite: Vec<u8>,
    partitioned: bool,
}

impl Options {
    fn read(
        context: &mut dyn Context,
        arguments: &[Value],
        function: &str,
    ) -> Result<Self, NativeError> {
        let Some(array) = args::array(arguments, 2) else {
            return Ok(Self {
                expires: args::int(arguments, 2).unwrap_or_default(),
                path: args::bytes(arguments, 3).to_vec(),
                domain: args::bytes(arguments, 4).to_vec(),
                secure: args::bool(arguments, 5).unwrap_or_default(),
                httponly: args::bool(arguments, 6).unwrap_or_default(),
                ..Self::default()
            });
        };
        if arguments.len() > 3 {
            return Err(NativeError::argument_count_error(format!(
                "{function}(): Expects exactly 3 arguments when argument #3 \
                 ($expires_or_options) is an array"
            )));
        }
        let mut options = Self::default();
        for (key, value) in array.iter() {
            let ArrayKey::String(key) = key else {
                return Err(NativeError::value_error(format!(
                    "{function}(): option array cannot have numeric keys"
                )));
            };
            let key_bytes = key.as_bytes().to_ascii_lowercase();
            match key_bytes.as_slice() {
                b"expires" => options.expires = to_int(value, context.diagnostics())?,
                b"path" => options.path = option_string(context, value)?,
                b"domain" => options.domain = option_string(context, value)?,
                b"samesite" => options.samesite = option_string(context, value)?,
                b"secure" => options.secure = to_bool(value),
                b"httponly" => options.httponly = to_bool(value),
                b"partitioned" => options.partitioned = to_bool(value),
                _ => {
                    return Err(NativeError::value_error(format!(
                        "{function}(): option \"{}\" is invalid",
                        String::from_utf8_lossy(c_string(key.as_bytes()))
                    )));
                }
            }
        }
        Ok(options)
    }

    fn validate(&self, function: &str) -> Result<(), NativeError> {
        for (label, value) in [("path", &self.path), ("domain", &self.domain)] {
            if illegal(value) {
                return Err(NativeError::value_error(format!(
                    "{function}(): \"{label}\" option cannot contain {INVALID}"
                )));
            }
        }
        if self.expires >= 253_402_300_800 {
            return Err(NativeError::value_error(format!(
                "{function}(): \"expires\" option cannot have a year greater than 9999"
            )));
        }
        if self.partitioned && !self.secure {
            return Err(NativeError::value_error(format!(
                "{function}(): \"partitioned\" option cannot be used without \"secure\" option"
            )));
        }
        Ok(())
    }

    fn append_to(&self, line: &mut Vec<u8>) {
        append_option(line, b"; path=", &self.path);
        append_option(line, b"; domain=", &self.domain);
        if self.secure {
            line.extend_from_slice(b"; secure");
        }
        if self.httponly {
            line.extend_from_slice(b"; HttpOnly");
        }
        append_option(line, b"; SameSite=", &self.samesite);
        if self.partitioned {
            line.extend_from_slice(b"; Partitioned");
        }
    }
}

fn c_string(value: &[u8]) -> &[u8] {
    value.split(|&byte| byte == 0).next().unwrap_or_default()
}

fn illegal(value: &[u8]) -> bool {
    c_string(value)
        .iter()
        .any(|byte| matches!(byte, b',' | b';' | b' ' | 9..=13))
}

fn option_string(context: &mut dyn Context, value: &Value) -> Result<Vec<u8>, NativeError> {
    if value.is_array() {
        context.warning("Array to string conversion".into());
        return Ok(b"Array".to_vec());
    }
    Ok(to_php_string(value, context.diagnostics())?
        .as_bytes()
        .to_vec())
}

fn append_option(line: &mut Vec<u8>, label: &[u8], value: &[u8]) {
    if !value.is_empty() {
        line.extend_from_slice(label);
        line.extend_from_slice(value);
    }
}

fn set_cookie(context: &mut dyn Context, arguments: &[Value], kind: Cookie) -> NativeResult {
    let function = kind.name();
    let name = args::bytes(arguments, 0);
    let value = args::bytes(arguments, 1);
    let options = Options::read(context, arguments, function)?;
    if name.is_empty() {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($name) must not be empty"
        )));
    }
    if illegal(name) || c_string(name).contains(&b'=') {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($name) cannot contain \"=\", {INVALID}"
        )));
    }
    if matches!(kind, Cookie::Raw) && illegal(value) {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #2 ($value) cannot contain {INVALID}"
        )));
    }
    options.validate(function)?;
    if !headers_open(context) {
        return Ok(Value::Bool(false));
    }

    let mut line = name.to_vec();
    line.push(b'=');
    if value.is_empty() {
        line.extend_from_slice(b"deleted; expires=Thu, 01 Jan 1970 00:00:01 GMT; Max-Age=0");
    } else {
        match kind {
            Cookie::Encoded => encode(&mut line, value),
            Cookie::Raw => line.extend_from_slice(value),
        }
        if options.expires > 0
            && let Some(date) = chrono::DateTime::from_timestamp(options.expires, 0)
        {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_secs() as i64);
            let age = (options.expires - now).max(0);
            let date = date.format("%a, %d %b %Y %H:%M:%S GMT");
            line.extend_from_slice(format!("; expires={date}; Max-Age={age}").as_bytes());
        }
    }
    options.append_to(&mut line);
    let line = line.trim_ascii_end();
    if !valid_header_line(context, line) {
        return Ok(Value::Bool(false));
    }
    context
        .state()
        .get_or_default::<ResponseHead>()
        .append_cookie(line);
    Ok(Value::Bool(true))
}

fn encode(line: &mut Vec<u8>, value: &[u8]) {
    const HEX: &[u8] = b"0123456789ABCDEF";
    for &byte in value {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            line.push(byte);
        } else {
            line.extend_from_slice(&[b'%', HEX[(byte >> 4) as usize], HEX[(byte & 15) as usize]]);
        }
    }
}
