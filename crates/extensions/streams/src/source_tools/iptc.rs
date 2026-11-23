//! `iptcparse($record)` decodes IPTC datasets. `iptcembed($record, 'photo.jpg')` uses JPEG APP13.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType,
    PhpArray, Registry, Value, args,
};

use crate::{
    errors::check_path,
    filtered_files,
    handles::{FILENAME, open_resource},
};

pub(super) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "iptcparse",
        const { &[Param::new("iptc_block", ParamType::String)] },
        |_, args| Ok(parse(args::bytes(args, 0))),
    ));
    registry.function(FunctionEntry::asynchronous(
        "iptcembed",
        const {
            &[
                Param::new("iptc_data", ParamType::String),
                FILENAME,
                Param::new("spool", ParamType::Int).with_default(ConstantValue::Int(0)),
            ]
        },
        embed,
    ));
    registry.describe_function("iptcparse", "array|false", Some("standard"));
    registry.describe_function("iptcembed", "string|bool", Some("standard"));
}

fn parse(mut bytes: &[u8]) -> Value {
    let mut tags = PhpArray::new();
    while let Some(start) = bytes
        .windows(2)
        .position(|pair| pair[0] == 0x1c && matches!(pair[1], 1 | 2))
    {
        bytes = &bytes[start..];
        let Some(header) = bytes.get(..5) else { break };
        let Some((length, offset)) =
            dataset_length(bytes, u16::from_be_bytes([header[3], header[4]]))
        else {
            break;
        };
        let Some(payload) = bytes.get(offset..).and_then(|rest| rest.get(..length)) else {
            break;
        };
        let key = ArrayKey::str(format!("{}#{:03}", header[1], header[2]));
        if !payload.is_empty() {
            let value = Value::from(payload.to_vec());
            if let Some(Value::Array(values)) = tags.get_mut(&key) {
                let _ = Rc::make_mut(values).push(value);
            } else {
                tags.insert(key, Value::from(PhpArray::from_values([value], 1)));
            }
        }
        bytes = &bytes[offset + length..];
    }
    if tags.is_empty() {
        Value::Bool(false)
    } else {
        Value::from(tags)
    }
}

fn dataset_length(bytes: &[u8], short: u16) -> Option<(usize, usize)> {
    if short & 0x8000 == 0 {
        return Some((usize::from(short), 5));
    }
    let width = usize::from(short & 0x7fff);
    if !(1..=4).contains(&width) {
        return None;
    }
    let offset = 5 + width;
    let length = bytes
        .get(5..offset)?
        .iter()
        .fold(0usize, |value, &byte| (value << 8) | usize::from(byte));
    Some((length, offset))
}

fn embed(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        const FUNCTION: &str = "iptcembed";
        let file = args::bytes(&arguments, 1);
        check_path(FUNCTION, "#2 ($filename)", file)?;
        let stream = open_resource(context, FUNCTION, file, b"rb", false, &Value::Null).await?;
        let Some(image) = filtered_files::contents(context, stream, FUNCTION, 0, None)
            .await?
            .to_php_string()
        else {
            return Ok(Value::Bool(false));
        };
        let metadata = args::bytes(&arguments, 0);
        let Some(image) = with_app13(image.as_bytes(), metadata) else {
            return Ok(Value::Bool(false));
        };
        let spool = args::int(&arguments, 2).unwrap_or_default();
        if spool > 0 {
            context.write(&image);
        }
        Ok(if spool >= 2 {
            Value::Bool(true)
        } else {
            Value::from(image)
        })
    })
}

fn with_app13(source: &[u8], metadata: &[u8]) -> Option<Vec<u8>> {
    const IDENTIFIER: &[u8] = b"Photoshop 3.0\0";
    const RESOURCE: &[u8] = b"8BIM\x04\x04\0\0";
    let padded = metadata.len().checked_add(metadata.len() % 2)?;
    let size =
        u16::try_from(padded.checked_add(2 + IDENTIFIER.len() + RESOURCE.len() + 4)?).ok()?;
    let mut remaining = source.strip_prefix(b"\xff\xd8")?;
    let mut output = Vec::from(&source[..2]);
    let mut inserted = false;
    let mut removed = false;
    while let [0xff, marker, ..] = remaining {
        if *marker == 0xda || (*marker == 0xd9 && removed) {
            output.extend_from_slice(remaining);
            break;
        }
        if *marker == 0xd9 {
            output.push(0xff);
            break;
        }
        let length: [u8; 2] = remaining.get(2..4)?.try_into().ok()?;
        let length = usize::from(u16::from_be_bytes(length));
        if length < 2 {
            return None;
        }
        let end = 2 + length;
        let segment = remaining.get(..end)?;
        if *marker == 0xed && !removed {
            removed = true;
        } else {
            output.extend_from_slice(segment);
        }
        if !inserted && matches!(*marker, 0xe0 | 0xe1) {
            output.extend_from_slice(b"\xff\xed");
            output.extend_from_slice(&size.to_be_bytes());
            output.extend_from_slice(IDENTIFIER);
            output.extend_from_slice(RESOURCE);
            output.extend_from_slice(&(padded as u32).to_be_bytes());
            output.extend_from_slice(metadata);
            if !metadata.len().is_multiple_of(2) {
                output.push(0);
            }
            inserted = true;
        }
        remaining = &remaining[end..];
    }
    Some(output)
}
