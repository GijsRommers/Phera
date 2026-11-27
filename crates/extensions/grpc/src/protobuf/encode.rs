//! PHP array to protobuf bytes.
//!
//! Fields are written in field-number order, whatever the order of the PHP
//! array, so equal arrays always give equal bytes. A nested message's length
//! comes before its bytes but is known only after them: one byte is
//! reserved, the message is written after it, and the length is patched in.
//! Only a message of 128 bytes or more needs a wider length, and then its
//! bytes move right by the extra width, once.

use std::borrow::Cow;
use std::fmt;

use phera_extension_api::{ArrayKey, PhpArray, Value};

use super::wire::{WireType, put_tag, put_varint, varint_bytes, varint_len, zigzag32, zigzag64};
use super::{FieldDef, Kind, Schema, flags};

/// The deepest nesting written, as in Google's implementations.
const MAX_DEPTH: u32 = 100;

/// Why a PHP array does not fit a message type: where, and what was wrong.
#[derive(Debug)]
pub struct EncodeError {
    /// Field names from the outer message inward, joined by dots.
    path: String,
    message: String,
}

impl EncodeError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            path: String::new(),
            message: message.into(),
        }
    }

    #[cold]
    fn within(mut self, segment: &str) -> Self {
        self.path = if self.path.is_empty() || self.path.starts_with('[') {
            format!("{segment}{}", self.path)
        } else {
            format!("{segment}.{}", self.path)
        };
        self
    }
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// Appends message `id` with the fields of `data` to `out`.
pub fn encode(
    schema: &Schema,
    id: u32,
    data: &PhpArray,
    out: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    Encoder { schema, out }
        .message(id, data, 0)
        .map_err(|error| error.within(&schema.message(id).name))
}

struct Encoder<'a> {
    schema: &'a Schema,
    out: &'a mut Vec<u8>,
}

/// A number or byte run, converted from PHP and ready to write.
enum Scalar<'v> {
    Varint(u64),
    Fixed32(u32),
    Fixed64(u64),
    Bytes(Cow<'v, [u8]>),
}

impl Scalar<'_> {
    /// A proto3 field without presence is not written at its default.
    fn is_default(&self) -> bool {
        match self {
            Self::Varint(value) | Self::Fixed64(value) => *value == 0,
            Self::Fixed32(value) => *value == 0,
            Self::Bytes(bytes) => bytes.is_empty(),
        }
    }

    #[inline]
    fn write(&self, out: &mut Vec<u8>) {
        match self {
            Self::Varint(value) => put_varint(out, *value),
            Self::Fixed32(value) => out.extend_from_slice(&value.to_le_bytes()),
            Self::Fixed64(value) => out.extend_from_slice(&value.to_le_bytes()),
            Self::Bytes(bytes) => {
                put_varint(out, bytes.len() as u64);
                out.extend_from_slice(bytes);
            }
        }
    }
}

impl Encoder<'_> {
    fn message(&mut self, id: u32, data: &PhpArray, depth: u32) -> Result<(), EncodeError> {
        if depth > MAX_DEPTH {
            return Err(EncodeError::new("messages are nested too deeply"));
        }
        let schema = self.schema;
        let mut matched = 0usize;
        for field in schema.fields(id) {
            let Some(value) = data.get(&field.key) else {
                continue;
            };
            matched += 1;
            let value = value.deref();
            if value.is_null() {
                continue;
            }
            self.field(field, &value, depth)
                .map_err(|error| error.within(&field.name()))?;
        }
        // Every key was a field unless fewer keys matched than the array has.
        if matched < data.len() {
            return Err(unknown_key(schema, id, data));
        }
        Ok(())
    }

    fn field(&mut self, field: &FieldDef, value: &Value, depth: u32) -> Result<(), EncodeError> {
        if field.has(flags::MAP) {
            return self.map(field, value, depth);
        }
        if field.has(flags::REPEATED) {
            return self.repeated(field, value, depth);
        }
        if field.kind == Kind::Message {
            return self.nested(field, value, depth);
        }
        let scalar = scalar(field, value)?;
        if field.has(flags::PRESENCE) || !scalar.is_default() {
            put_tag(self.out, field.number, field.kind.wire_type());
            scalar.write(self.out);
        }
        Ok(())
    }

    fn nested(&mut self, field: &FieldDef, value: &Value, depth: u32) -> Result<(), EncodeError> {
        let data = message_array(value)?;
        put_tag(self.out, field.number, WireType::Len);
        let start = self.begin_len();
        self.message(field.message, &data, depth + 1)?;
        self.end_len(start);
        Ok(())
    }

    fn repeated(&mut self, field: &FieldDef, value: &Value, depth: u32) -> Result<(), EncodeError> {
        let Value::Array(items) = value else {
            return Err(expected("an array", value));
        };
        if field.has(flags::PACKED) {
            if items.is_empty() {
                return Ok(());
            }
            put_tag(self.out, field.number, WireType::Len);
            let start = self.begin_len();
            for (index, (_, item)) in items.iter().enumerate() {
                scalar(field, &item.deref())
                    .map_err(|error| error.within(&format!("[{index}]")))?
                    .write(self.out);
            }
            self.end_len(start);
            return Ok(());
        }
        for (index, (_, item)) in items.iter().enumerate() {
            let item = item.deref();
            let result = if field.kind == Kind::Message {
                self.nested(field, &item, depth)
            } else {
                scalar(field, &item).map(|scalar| {
                    put_tag(self.out, field.number, field.kind.wire_type());
                    scalar.write(self.out);
                })
            };
            result.map_err(|error| error.within(&format!("[{index}]")))?;
        }
        Ok(())
    }

    fn map(&mut self, field: &FieldDef, value: &Value, depth: u32) -> Result<(), EncodeError> {
        let Value::Array(entries) = value else {
            return Err(expected("an array", value));
        };
        let entry_fields = self.schema.fields(field.message);
        let (key_field, value_field) = (&entry_fields[0], &entry_fields[1]);
        for (key, item) in entries.iter() {
            let item = item.deref();
            let at = || match &key {
                ArrayKey::Int(number) => format!("[{number}]"),
                ArrayKey::Interned(name) => format!("[{}]", name.to_string_lossy()),
                ArrayKey::String(name) => format!("[{}]", name.to_string_lossy()),
            };
            put_tag(self.out, field.number, WireType::Len);
            let start = self.begin_len();
            let key = map_key(key_field, &key).map_err(|error| error.within(&at()))?;
            put_tag(self.out, 1, key_field.kind.wire_type());
            key.write(self.out);
            // Entries always carry their value, even a default one.
            let written = if value_field.kind == Kind::Message {
                if item.is_null() {
                    Ok(())
                } else {
                    self.nested(value_field, &item, depth)
                }
            } else {
                scalar(value_field, &item).map(|scalar| {
                    put_tag(self.out, 2, value_field.kind.wire_type());
                    scalar.write(self.out);
                })
            };
            written.map_err(|error| error.within(&at()))?;
            self.end_len(start);
        }
        Ok(())
    }

    /// Reserves the one length byte that almost every nested message needs.
    #[inline]
    fn begin_len(&mut self) -> usize {
        self.out.push(0);
        self.out.len()
    }

    /// Writes the length of everything after `start`, widening its slot when
    /// the length needs more than one byte.
    #[inline]
    fn end_len(&mut self, start: usize) {
        let len = self.out.len() - start;
        if len < 0x80 {
            self.out[start - 1] = len as u8;
            return;
        }
        let width = varint_len(len as u64);
        let end = self.out.len();
        self.out.resize(end + width - 1, 0);
        self.out.copy_within(start..end, start + width - 1);
        let (prefix, width) = varint_bytes(len as u64);
        self.out[start - 1..start - 1 + width].copy_from_slice(&prefix[..width]);
    }
}

fn message_array(value: &Value) -> Result<Cow<'_, PhpArray>, EncodeError> {
    match value {
        Value::Array(array) => Ok(Cow::Borrowed(array)),
        // A `stdClass` from json_decode() works as well.
        Value::Object(object) => Ok(Cow::Owned(object.to_array())),
        _ => Err(expected("an array", value)),
    }
}

/// Converts a PHP value for a field that is not a message.
fn scalar<'v>(field: &FieldDef, value: &'v Value) -> Result<Scalar<'v>, EncodeError> {
    Ok(match field.kind {
        Kind::Int32 | Kind::Enum => Scalar::Varint(int32(value)? as i64 as u64),
        Kind::Sint32 => Scalar::Varint(u64::from(zigzag32(int32(value)?))),
        Kind::Sfixed32 => Scalar::Fixed32(int32(value)? as u32),
        Kind::Uint32 => Scalar::Varint(u64::from(uint32(value)?)),
        Kind::Fixed32 => Scalar::Fixed32(uint32(value)?),
        Kind::Int64 => Scalar::Varint(integer(value)? as u64),
        Kind::Sint64 => Scalar::Varint(zigzag64(integer(value)?)),
        Kind::Sfixed64 => Scalar::Fixed64(integer(value)? as u64),
        Kind::Uint64 => Scalar::Varint(uint64(value)?),
        Kind::Fixed64 => Scalar::Fixed64(uint64(value)?),
        Kind::Bool => Scalar::Varint(u64::from(boolean(value)?)),
        Kind::Double => Scalar::Fixed64(float(value)?.to_bits()),
        Kind::Float => Scalar::Fixed32((float(value)? as f32).to_bits()),
        Kind::String => {
            let bytes = bytes(value)?;
            if field.has(flags::UTF8) && std::str::from_utf8(&bytes).is_err() {
                return Err(EncodeError::new("string is not valid UTF-8"));
            }
            Scalar::Bytes(bytes)
        }
        Kind::Bytes => Scalar::Bytes(bytes(value)?),
        Kind::Message => return Err(EncodeError::new("expected a message")),
    })
}

/// A map key arrives as a PHP array key: integers stay integers, and
/// numeric-looking strings have already become integers.
fn map_key(field: &FieldDef, key: &ArrayKey) -> Result<Scalar<'static>, EncodeError> {
    let value = match key {
        ArrayKey::Int(number) if field.kind == Kind::String => Value::from(number.to_string()),
        ArrayKey::Int(number) => Value::Int(*number),
        ArrayKey::Interned(name) => Value::InternedStr(name),
        ArrayKey::String(name) => Value::String(name.clone()),
    };
    if field.kind == Kind::Bool
        && let Some(name) = value.as_str_bytes()
    {
        return match name {
            b"true" => Ok(Scalar::Varint(1)),
            b"false" => Ok(Scalar::Varint(0)),
            _ => Err(EncodeError::new("expected a boolean map key")),
        };
    }
    Ok(match scalar(field, &value)? {
        Scalar::Bytes(bytes) => Scalar::Bytes(Cow::Owned(bytes.into_owned())),
        Scalar::Varint(number) => Scalar::Varint(number),
        Scalar::Fixed32(number) => Scalar::Fixed32(number),
        Scalar::Fixed64(number) => Scalar::Fixed64(number),
    })
}

/// -2^63 and 2^63, exactly representable as floats.
const I64_MIN: f64 = i64::MIN as f64;
const I64_END: f64 = -(i64::MIN as f64);

fn integer(value: &Value) -> Result<i64, EncodeError> {
    match value {
        Value::Int(number) => Ok(*number),
        Value::Bool(flag) => Ok(i64::from(*flag)),
        Value::Float(number) if number.fract() == 0.0 && (I64_MIN..I64_END).contains(number) => {
            Ok(*number as i64)
        }
        Value::InternedStr(_) | Value::String(_) => {
            let text = value.as_str_bytes().unwrap_or_default();
            std::str::from_utf8(text)
                .ok()
                .and_then(|text| text.trim().parse::<i64>().ok())
                .ok_or_else(|| expected("an integer", value))
        }
        _ => Err(expected("an integer", value)),
    }
}

fn int32(value: &Value) -> Result<i32, EncodeError> {
    let number = integer(value)?;
    i32::try_from(number)
        .map_err(|_| EncodeError::new(format!("{number} is out of range for a 32-bit integer")))
}

fn uint32(value: &Value) -> Result<u32, EncodeError> {
    let number = integer(value)?;
    u32::try_from(number).map_err(|_| {
        EncodeError::new(format!(
            "{number} is out of range for an unsigned 32-bit integer"
        ))
    })
}

/// PHP has no unsigned 64-bit integer: a negative int is taken as its two's
/// complement bit pattern (what decoding returns for values above
/// `PHP_INT_MAX`), and a decimal string may spell any value up to 2^64 - 1.
fn uint64(value: &Value) -> Result<u64, EncodeError> {
    if let Some(text) = value.as_str_bytes()
        && let Some(number) = std::str::from_utf8(text)
            .ok()
            .and_then(|text| text.trim().parse::<u64>().ok())
    {
        return Ok(number);
    }
    integer(value).map(|number| number as u64)
}

fn float(value: &Value) -> Result<f64, EncodeError> {
    match value {
        Value::Float(number) => Ok(*number),
        Value::Int(number) => Ok(*number as f64),
        Value::Bool(flag) => Ok(f64::from(u8::from(*flag))),
        Value::InternedStr(_) | Value::String(_) => {
            let text = value.as_str_bytes().unwrap_or_default();
            std::str::from_utf8(text)
                .ok()
                .and_then(|text| text.trim().parse::<f64>().ok())
                .ok_or_else(|| expected("a number", value))
        }
        _ => Err(expected("a number", value)),
    }
}

fn boolean(value: &Value) -> Result<bool, EncodeError> {
    match value {
        Value::Bool(flag) => Ok(*flag),
        Value::Int(number) => Ok(*number != 0),
        Value::Float(number) => Ok(*number != 0.0),
        Value::InternedStr(_) | Value::String(_) => Ok(!matches!(
            value.as_str_bytes().unwrap_or_default(),
            b"" | b"0"
        )),
        _ => Err(expected("a boolean", value)),
    }
}

fn bytes(value: &Value) -> Result<Cow<'_, [u8]>, EncodeError> {
    match value {
        Value::InternedStr(_) | Value::String(_) => {
            Ok(Cow::Borrowed(value.as_str_bytes().unwrap_or_default()))
        }
        Value::Int(number) => Ok(Cow::Owned(number.to_string().into_bytes())),
        _ => Err(expected("a string", value)),
    }
}

#[cold]
fn expected(what: &str, value: &Value) -> EncodeError {
    EncodeError::new(format!("expected {what}, got {}", value.type_name()))
}

#[cold]
fn unknown_key(schema: &Schema, id: u32, data: &PhpArray) -> EncodeError {
    let fields = schema.fields(id);
    let unknown = data
        .keys()
        .find(|key| !fields.iter().any(|field| field.key == *key));
    let name = match unknown {
        Some(ArrayKey::Int(number)) => number.to_string(),
        Some(ArrayKey::Interned(name)) => name.to_string_lossy(),
        Some(ArrayKey::String(name)) => name.to_string_lossy(),
        None => String::new(),
    };
    EncodeError::new(format!(
        "message {} has no field \"{name}\"",
        schema.message(id).name
    ))
}
