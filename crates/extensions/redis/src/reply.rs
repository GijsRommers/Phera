//! Redis nil becomes PHP false; associative and serialized replies are method-specific.
mod raw;
mod streams;
use crate::options::{Options, deserialize};
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value};
use redis::Value as Reply;

#[derive(Clone)]
pub enum Shape {
    Raw,
    Literal,
    Value,
    Values,
    Bool,
    Float,
    Floats,
    Type,
    Pairs,
    RawPairs,
    Scores,
    Info,
    StreamInfo,
    Fields(Vec<Vec<u8>>),
    Pop { sorted: bool },
    MultiPop { sorted: bool },
    Entries,
    Streams,
    AutoClaim { just_id: bool },
    Coordinates,
}

pub async fn convert(
    context: &mut dyn Context,
    options: &Options,
    shape: &Shape,
    reply: Reply,
) -> NativeResult {
    if matches!(reply, Reply::Nil) && matches!(shape, Shape::Pop { .. } | Shape::Streams) {
        return Ok(if options.null_multibulk {
            Value::Null
        } else {
            Value::from(PhpArray::new())
        });
    }
    if matches!(
        reply,
        Reply::Nil | Reply::Boolean(false) | Reply::ServerError(_)
    ) {
        return raw::convert(reply, options);
    }
    match shape {
        Shape::Literal => raw::convert(reply, options),
        Shape::Raw => raw::convert(
            reply,
            &Options {
                reply_literal: false,
                ..options.clone()
            },
        ),
        Shape::Bool => Ok(Value::Bool(
            matches!(reply, Reply::Int(i) if i != 0)
                || matches!(reply, Reply::Okay | Reply::Boolean(true)),
        )),
        Shape::Value => value(context, options, reply).await,
        Shape::Float => float(reply),
        Shape::Type => Ok(Value::Int(match bytes(reply).as_slice() {
            b"string" => 1,
            b"set" => 2,
            b"list" => 3,
            b"zset" => 4,
            b"hash" => 5,
            b"stream" => 6,
            b"vectorset" => 7,
            _ => 0,
        })),
        Shape::Values | Shape::Floats => {
            let mut result = PhpArray::new();
            for item in items(reply) {
                result.push(if matches!(shape, Shape::Floats) {
                    float(item)?
                } else {
                    value(context, options, item).await?
                })?;
            }
            Ok(Value::from(result))
        }
        Shape::Pairs | Shape::Scores | Shape::RawPairs => {
            pairs(
                context,
                options,
                reply,
                matches!(shape, Shape::Scores),
                matches!(shape, Shape::Pairs),
            )
            .await
        }
        Shape::Fields(fields) => {
            let mut result = PhpArray::new();
            for (field, item) in fields.iter().zip(items(reply)) {
                result.insert(key(field.clone())?, value(context, options, item).await?);
            }
            Ok(Value::from(result))
        }
        Shape::Pop { sorted } => {
            let mut result = PhpArray::new();
            for (index, item) in items(reply).into_iter().enumerate() {
                result.push(match index {
                    1 => value(context, options, item).await?,
                    2 if *sorted => raw::convert(item, options)?,
                    _ => raw::convert(item, options)?,
                })?;
            }
            Ok(Value::from(result))
        }
        Shape::MultiPop { sorted } => {
            let mut parts = items(reply).into_iter();
            let mut result = PhpArray::new();
            if let Some(key) = parts.next() {
                result.push(raw::convert(key, options)?)?;
            }
            if let Some(values) = parts.next() {
                let mut entries = PhpArray::new();
                for item in items(values) {
                    if *sorted {
                        let mut pair = items(item).into_iter();
                        let member = pair.next().unwrap_or(Reply::Nil);
                        let score = pair.next().unwrap_or(Reply::Nil);
                        entries.insert(key(bytes(member))?, float(score)?);
                    } else {
                        entries.push(value(context, options, item).await?)?;
                    }
                }
                result.push(Value::from(entries))?;
            }
            Ok(Value::from(result))
        }
        Shape::Info => info(bytes(reply)),
        Shape::StreamInfo => streams::info(context, options, reply).await,
        Shape::Entries => streams::entries(context, options, reply).await,
        Shape::Streams => streams::streams(context, options, reply).await,
        Shape::AutoClaim { just_id } => streams::claim(context, options, reply, *just_id).await,
        Shape::Coordinates => {
            let mut result = PhpArray::new();
            for coordinates in items(reply) {
                if matches!(coordinates, Reply::Nil) {
                    result.push(Value::from(PhpArray::new()))?;
                } else {
                    let mut pair = PhpArray::new();
                    for coordinate in items(coordinates) {
                        pair.push(raw::convert(coordinate, options)?)?;
                    }
                    result.push(Value::from(pair))?;
                }
            }
            Ok(Value::from(result))
        }
    }
}

async fn value(context: &mut dyn Context, options: &Options, reply: Reply) -> NativeResult {
    match reply {
        Reply::BulkString(bytes) => deserialize(context, options, bytes).await,
        other => raw::convert(other, options),
    }
}

pub fn items(reply: Reply) -> Vec<Reply> {
    match reply {
        Reply::Array(items) | Reply::Set(items) => items,
        Reply::Map(items) => items.into_iter().flat_map(|(k, v)| [k, v]).collect(),
        _ => Vec::new(),
    }
}

pub fn bytes(reply: Reply) -> Vec<u8> {
    match reply {
        Reply::BulkString(b) | Reply::BigNumber(b) => b,
        Reply::SimpleString(s) => s.into_bytes(),
        Reply::Int(i) => i.to_string().into_bytes(),
        Reply::Double(f) => f.to_string().into_bytes(),
        _ => Vec::new(),
    }
}

fn float(reply: Reply) -> NativeResult {
    Ok(match reply {
        Reply::Nil => Value::Bool(false),
        Reply::Double(f) => Value::Float(f),
        Reply::Int(i) => Value::Float(i as f64),
        other => Value::Float(
            String::from_utf8_lossy(&bytes(other))
                .parse()
                .unwrap_or(0.0),
        ),
    })
}

fn key(bytes: Vec<u8>) -> Result<ArrayKey, NativeError> {
    Ok(ArrayKey::from_value(&Value::from(bytes))?)
}

async fn pairs(
    c: &mut dyn Context,
    o: &Options,
    reply: Reply,
    scores: bool,
    unpack: bool,
) -> NativeResult {
    let mut result = PhpArray::new();
    let mut values = items(reply).into_iter();
    while let Some(k) = values.next() {
        // RESP3 score pairs are nested; RESP2 pairs are flat.
        let (k, v) = if let Reply::Array(pair) = k {
            let mut pair = pair.into_iter();
            (
                pair.next().unwrap_or(Reply::Nil),
                pair.next().unwrap_or(Reply::Nil),
            )
        } else {
            (k, values.next().unwrap_or(Reply::Nil))
        };
        let v = if scores {
            float(v)?
        } else if unpack {
            value(c, o, v).await?
        } else {
            raw::convert(v, o)?
        };
        result.insert(key(bytes(k))?, v);
    }
    Ok(Value::from(result))
}

fn info(bytes: Vec<u8>) -> NativeResult {
    let mut result = PhpArray::new();
    for line in String::from_utf8_lossy(&bytes).lines() {
        if line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = value
                .parse::<i64>()
                .map(Value::Int)
                .or_else(|_| value.parse::<f64>().map(Value::Float))
                .unwrap_or_else(|_| Value::from(value));
            result.insert(ArrayKey::str(key), value);
        }
    }
    Ok(Value::from(result))
}
