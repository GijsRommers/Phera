//! A serialized array of scalars can be assembled directly into PHP arrays.

use std::rc::Rc;

use phera_extension_api::serialization::UnserializePolicy;
use phera_extension_api::{ArrayKey, Context, NativeResult, PhpArray, Value};

use super::{emit_diagnostic, php_string};
use crate::wire::{ParseError, Reader, Token};

/// Without object or reference syntax, no later value can change an earlier
/// array entry or require a PHP call. A tag inside a string can only opt out.
pub(super) fn applies(bytes: &[u8]) -> bool {
    match bytes.first() {
        Some(b'a') => !super::has_wire_tags(bytes, &[b"O:", b"C:", b"E:", b"R:", b"r:"]),
        Some(b'O' | b'C' | b'E' | b'R' | b'r') => false,
        _ => true,
    }
}

struct Frame {
    array: PhpArray,
    remaining: usize,
    key: Option<ArrayKey>,
}

pub(super) fn read(
    context: &mut dyn Context,
    bytes: &[u8],
    policy: &UnserializePolicy,
) -> NativeResult {
    let mut reader = Reader::new(bytes);
    match build(context, &mut reader, policy) {
        Ok(value) => {
            super::report_extra(context, reader.offset(), bytes.len());
            Ok(value)
        }
        Err(_) => {
            super::report_offset(context, reader.offset(), bytes.len());
            Ok(Value::Bool(false))
        }
    }
}

fn build(
    context: &mut dyn Context,
    reader: &mut Reader<'_>,
    policy: &UnserializePolicy,
) -> Result<Value, ParseError> {
    let mut frames: Vec<Frame> = Vec::new();
    loop {
        if let Some(frame) = frames.last_mut() {
            let key = read_token(context, reader, true)?;
            frame.key = Some(match key {
                Token::Int(key) => ArrayKey::Int(key),
                Token::String(key) => {
                    ArrayKey::from_value(&Value::String(php_string(key))).map_err(|_| ParseError)?
                }
                _ => return Err(ParseError),
            });
        }
        let mut value = match read_token(context, reader, false)? {
            Token::Null => Value::Null,
            Token::Bool(value) => Value::Bool(value),
            Token::Int(value) => Value::Int(value),
            Token::Float(value) => Value::Float(value),
            Token::String(value) => Value::String(php_string(value)),
            Token::Array(0) => {
                reader.close()?;
                Value::Array(Rc::default())
            }
            Token::Array(remaining) => {
                if policy.depth_exceeded(frames.len()) {
                    context.warning(policy.depth_warning("unserialize"));
                    return Err(ParseError);
                }
                frames.push(Frame {
                    array: PhpArray::new(),
                    remaining,
                    key: None,
                });
                continue;
            }
            _ => return Err(ParseError),
        };
        loop {
            let Some(frame) = frames.last_mut() else {
                return Ok(value);
            };
            let key = frame.key.take().ok_or(ParseError)?;
            frame.array.insert(key, value);
            frame.remaining -= 1;
            if frame.remaining != 0 {
                break;
            }
            reader.close()?;
            let frame = frames.pop().ok_or(ParseError)?;
            value = Value::Array(Rc::new(frame.array));
        }
    }
}

fn read_token<'a>(
    context: &mut dyn Context,
    reader: &mut Reader<'a>,
    key: bool,
) -> Result<Token<'a>, ParseError> {
    let token = reader.token(key);
    emit_diagnostic(context, reader);
    token
}
