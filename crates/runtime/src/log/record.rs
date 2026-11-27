//! One log record: its level, its message with `{placeholders}` filled in,
//! and the line it becomes in JSON or text.
//!
//! ```text
//! {"ts":"2026-11-27T09:15:02.123456+00:00","level":"info","channel":"app","message":"User 7 signed in","context":{"user":7}}
//! [2026-11-27T09:15:02.123456+00:00] app.INFO: User 7 signed in {"user":7}
//! ```

use std::rc::Rc;
use std::time::SystemTime;

use phera_extension_api::{ArrayKey, Context, Object, PhpArray, PhpString, Value};

/// The eight PSR-3 levels, most severe first, numbered as RFC 5424 does.
/// A logger writes a record when `record.level <= logger.min_level`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Level {
    Emergency = 0,
    Alert = 1,
    Critical = 2,
    Error = 3,
    Warning = 4,
    Notice = 5,
    Info = 6,
    Debug = 7,
}

impl Level {
    pub(super) const ALL: [Level; 8] = [
        Level::Emergency,
        Level::Alert,
        Level::Critical,
        Level::Error,
        Level::Warning,
        Level::Notice,
        Level::Info,
        Level::Debug,
    ];

    /// `"warning"`, as PSR-3's `LogLevel::WARNING`.
    pub(super) fn name(self) -> &'static str {
        match self {
            Level::Emergency => "emergency",
            Level::Alert => "alert",
            Level::Critical => "critical",
            Level::Error => "error",
            Level::Warning => "warning",
            Level::Notice => "notice",
            Level::Info => "info",
            Level::Debug => "debug",
        }
    }

    /// `"WARNING"`, for text lines.
    fn upper_name(self) -> &'static str {
        match self {
            Level::Emergency => "EMERGENCY",
            Level::Alert => "ALERT",
            Level::Critical => "CRITICAL",
            Level::Error => "ERROR",
            Level::Warning => "WARNING",
            Level::Notice => "NOTICE",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
        }
    }

    /// A level from its name, in any case.
    pub(super) fn parse(name: &[u8]) -> Option<Level> {
        Level::ALL
            .into_iter()
            .find(|level| level.name().as_bytes().eq_ignore_ascii_case(name))
    }
}

/// How a record is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Format {
    /// One JSON object per line (NDJSON).
    Json,
    /// `[time] channel.LEVEL: message {context}`.
    Text,
}

impl Format {
    pub(super) fn parse(name: &[u8]) -> Option<Format> {
        if name.eq_ignore_ascii_case(b"json") {
            Some(Format::Json)
        } else if name.eq_ignore_ascii_case(b"text") {
            Some(Format::Text)
        } else {
            None
        }
    }
}

/// `json_encode` flags for records: text as it is, invalid UTF-8 replaced
/// by U+FFFD, and never a failure (a log line must not throw).
const JSON_FLAGS: i64 = phera_ext_json::flag::UNESCAPED_SLASHES
    | phera_ext_json::flag::UNESCAPED_UNICODE
    | phera_ext_json::flag::INVALID_UTF8_SUBSTITUTE
    | phera_ext_json::flag::PARTIAL_OUTPUT_ON_ERROR;

/// How deep context arrays are written; deeper ones become `"[...]"`.
const MAX_DEPTH: usize = 16;

/// A record ready to be written or kept: everything in it is plain data.
pub(super) struct Record {
    pub(super) time: String,
    pub(super) level: Level,
    pub(super) message: PhpString,
    /// The merged context, with objects turned into plain values.
    pub(super) context: Value,
}

impl Record {
    /// `message` with its placeholders filled from `context`, which must
    /// already be the merged context the record keeps.
    pub(super) fn new(
        level: Level,
        message: &[u8],
        context: &Rc<PhpArray>,
        diagnostics: &mut dyn Context,
    ) -> Record {
        let message = interpolate(message, context, diagnostics);
        let context = normalize(&Value::Array(context.clone()), 0);
        Record {
            time: now(),
            level,
            message,
            context,
        }
    }

    /// The line this record becomes, with its trailing newline.
    pub(super) fn line(&self, channel: &str, format: Format, context: &mut dyn Context) -> Vec<u8> {
        let context_json = self.context_json(context);
        let mut line = Vec::with_capacity(96 + self.message.len() + context_json.len());
        match format {
            Format::Json => {
                let mut head = PhpArray::with_capacity(4);
                head.insert(ArrayKey::str("ts"), Value::from(self.time.as_str()));
                head.insert(ArrayKey::str("level"), Value::from(self.level.name()));
                head.insert(ArrayKey::str("channel"), Value::from(channel));
                head.insert(
                    ArrayKey::str("message"),
                    Value::String(self.message.clone()),
                );
                let head = encode(&Value::from(head), context);
                // `{"ts":...,"message":"..."}` + `,"context":{...}}`
                line.extend_from_slice(head.strip_suffix(b"}").unwrap_or(&head));
                line.extend_from_slice(b",\"context\":");
                line.extend_from_slice(&context_json);
                line.push(b'}');
            }
            Format::Text => {
                line.push(b'[');
                line.extend_from_slice(self.time.as_bytes());
                line.extend_from_slice(b"] ");
                line.extend_from_slice(channel.as_bytes());
                line.push(b'.');
                line.extend_from_slice(self.level.upper_name().as_bytes());
                line.extend_from_slice(b": ");
                line.extend_from_slice(self.message.as_bytes());
                if context_json != b"{}" {
                    line.push(b' ');
                    line.extend_from_slice(&context_json);
                }
            }
        }
        line.push(b'\n');
        line
    }

    /// The context as JSON; an empty context is `{}`.
    fn context_json(&self, context: &mut dyn Context) -> Vec<u8> {
        match &self.context {
            Value::Array(array) if array.is_empty() => b"{}".to_vec(),
            other => encode(other, context),
        }
    }
}

/// The current time as ISO 8601 in UTC with microseconds:
/// `2026-11-27T09:15:02.123456+00:00`.
fn now() -> String {
    chrono::DateTime::<chrono::Utc>::from(SystemTime::now())
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
}

fn encode(value: &Value, context: &mut dyn Context) -> Vec<u8> {
    match phera_ext_json::encode_plain(context, value, JSON_FLAGS) {
        Some(Ok(Value::String(json))) => json.into_bytes(),
        Some(Ok(other)) => other.as_str_bytes().unwrap_or(b"null").to_vec(),
        // Values are normalized first, so neither happens; write `null`
        // rather than lose the record.
        Some(Err(_)) | None => b"null".to_vec(),
    }
}

/// PSR-3 section 1.2: `{name}` is replaced by `context['name']` when the
/// context has that key; names are `A-Z a-z 0-9 _ .`. Other braces stay.
fn interpolate(message: &[u8], context: &PhpArray, diagnostics: &mut dyn Context) -> PhpString {
    if context.is_empty() || memchr::memchr(b'{', message).is_none() {
        return PhpString::from(message);
    }
    let is_name_byte = |byte: &u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.');
    let mut out = Vec::with_capacity(message.len() + 16);
    let mut rest = message;
    while let Some(open) = memchr::memchr(b'{', rest) {
        out.extend_from_slice(&rest[..open]);
        let after = &rest[open + 1..];
        let name_length = after.iter().take_while(|byte| is_name_byte(byte)).count();
        let closed = after.get(name_length) == Some(&b'}');
        let value = (closed && name_length > 0)
            .then(|| context.get_str_bytes(&after[..name_length]))
            .flatten();
        match value {
            Some(value) => {
                placeholder_text(&value.deref(), &mut out, diagnostics);
                rest = &after[name_length + 1..];
            }
            None => {
                out.push(b'{');
                rest = after;
            }
        }
    }
    out.extend_from_slice(rest);
    PhpString::from(out)
}

/// What a context value reads as inside a message: scalars as PHP's
/// `(string)` cast writes them, arrays as `array` + JSON, a throwable as
/// `Class: message`, other objects as `[object Class]`.
fn placeholder_text(value: &Value, out: &mut Vec<u8>, diagnostics: &mut dyn Context) {
    match value {
        Value::Null | Value::Bool(false) => {}
        Value::Bool(true) => out.push(b'1'),
        Value::Int(number) => out.extend_from_slice(number.to_string().as_bytes()),
        Value::Float(number) => out.extend_from_slice(
            phera_extension_api::value::float_to_string_precision14(*number).as_bytes(),
        ),
        Value::InternedStr(_) | Value::String(_) => {
            out.extend_from_slice(value.as_str_bytes().unwrap_or_default());
        }
        Value::Array(_) => {
            out.extend_from_slice(b"array");
            out.extend_from_slice(&encode(&normalize(value, 0), diagnostics));
        }
        Value::Object(object) if object.class.is_instance_of("Throwable") => {
            out.extend_from_slice(object.class.name().as_bytes());
            out.extend_from_slice(b": ");
            if let Some(message) = object.get(&Object::protected_key("message")) {
                out.extend_from_slice(message.as_str_bytes().unwrap_or_default());
            }
        }
        Value::Object(object) => {
            out.extend_from_slice(b"[object ");
            out.extend_from_slice(object.class.name().as_bytes());
            out.push(b']');
        }
        Value::Resource(_) => out.extend_from_slice(b"[resource]"),
        Value::Reference(cell) => placeholder_text(&cell.borrow(), out, diagnostics),
    }
}

/// A context value as plain data that `json_encode` writes without running
/// PHP: a throwable becomes its class, message, code, file and line (with
/// its previous one), any other object `[object Class]`, `NAN` and `INF`
/// strings. Objects that need PHP (`__toString`, `jsonSerialize`) were
/// resolved before; see [`needs_php`].
pub(super) fn normalize(value: &Value, depth: usize) -> Value {
    match value {
        Value::Float(number) if !number.is_finite() => Value::from(if number.is_nan() {
            "NAN"
        } else if *number > 0.0 {
            "INF"
        } else {
            "-INF"
        }),
        Value::Array(array) if depth >= MAX_DEPTH && !array.is_empty() => Value::from("[...]"),
        Value::Array(array) if array.values().all(is_plain_scalar) => value.clone(),
        Value::Array(array) => {
            let mut plain = PhpArray::with_capacity(array.len());
            for (key, item) in array.iter() {
                plain.insert(key, normalize(&item.deref(), depth + 1));
            }
            Value::from(plain)
        }
        Value::Object(object) if object.class.is_instance_of("Throwable") => {
            throwable(object, depth)
        }
        Value::Object(object) => Value::from(format!("[object {}]", object.class.name())),
        Value::Resource(_) => Value::from("[resource]"),
        Value::Reference(cell) => normalize(&cell.borrow(), depth),
        other => other.clone(),
    }
}

fn is_plain_scalar(value: &Value) -> bool {
    match value {
        Value::Float(number) => number.is_finite(),
        Value::Null | Value::Bool(_) | Value::Int(_) | Value::InternedStr(_) | Value::String(_) => {
            true
        }
        _ => false,
    }
}

/// `{"class":"RuntimeException","message":"...","code":0,"file":"...","line":12}`
fn throwable(object: &Object, depth: usize) -> Value {
    let mut fields = PhpArray::with_capacity(6);
    fields.insert(ArrayKey::str("class"), Value::from(object.class.name()));
    for name in ["message", "code", "file", "line"] {
        let value = object
            .get(&Object::protected_key(name))
            .map_or(Value::Null, |value| normalize(&value, depth + 1));
        fields.insert(ArrayKey::str(name), value);
    }
    let previous = ["Exception", "Error"]
        .into_iter()
        .find_map(|class| object.get(&Object::private_key(class, "previous")));
    if let Some(previous @ Value::Object(_)) = previous
        && depth < MAX_DEPTH
    {
        fields.insert(ArrayKey::str("previous"), normalize(&previous, depth + 1));
    }
    Value::from(fields)
}

/// Whether writing `value` needs PHP to run first: a `Stringable` or
/// `JsonSerializable` object, or a date, anywhere inside it. A throwable
/// never does; it is written from its properties.
pub(super) fn needs_php(value: &Value, context: &dyn Context, depth: usize) -> bool {
    match value {
        Value::Array(array) if depth < MAX_DEPTH => array
            .values()
            .any(|item| needs_php(&item.deref(), context, depth + 1)),
        Value::Object(object) => resolution(object, context).is_some(),
        Value::Reference(cell) => needs_php(&cell.borrow(), context, depth),
        _ => false,
    }
}

/// How PHP turns an object into plain data for a record.
pub(super) enum Resolution {
    /// `$object->format(DATE_RFC3339_EXTENDED)`.
    Date,
    /// `$object->jsonSerialize()`.
    Json,
    /// `(string) $object`.
    String,
}

pub(super) fn resolution(object: &Object, context: &dyn Context) -> Option<Resolution> {
    if object.class.is_instance_of("Throwable") {
        None
    } else if object.class.is_instance_of("DateTimeInterface") {
        Some(Resolution::Date)
    } else if object.class.is_instance_of("JsonSerializable") {
        Some(Resolution::Json)
    } else if context.has_method(object, "__toString") {
        Some(Resolution::String)
    } else {
        None
    }
}
