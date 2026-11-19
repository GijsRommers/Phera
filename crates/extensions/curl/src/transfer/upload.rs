//! `CURLOPT_POSTFIELDS` accepts bytes or multipart parts. `CURLOPT_INFILE` reads a PHP stream.

use std::rc::Rc;

use bytes::Bytes;
use phera_extension_api::{ArrayKey, Context, Object, PhpArray, Value};

use crate::{
    error::TransferError,
    options::{Body, Method, Settings},
};

#[derive(Clone, Default)]
pub(super) struct Payload {
    pub(super) bytes: Bytes,
    pub(super) content_type: Option<String>,
    pub(super) has_body: bool,
}

pub(super) async fn build(
    context: &mut dyn Context,
    object: &Rc<Object>,
    settings: &Settings,
) -> Result<Result<Payload, TransferError>, phera_extension_api::NativeError> {
    let payload = match (settings.method, &settings.body) {
        (Method::Post, Some(Body::Text(bytes))) => Payload {
            bytes: Bytes::copy_from_slice(bytes),
            content_type: Some("application/x-www-form-urlencoded".to_owned()),
            has_body: true,
        },
        (Method::Post, Some(Body::Multipart(fields))) => {
            return multipart(context, fields).await;
        }
        (Method::Put, _) => return read(context, object, settings).await,
        (Method::Post, None) if settings.read.is_some() || settings.input.is_some() => {
            return read(context, object, settings).await;
        }
        (Method::Post, None) => Payload {
            content_type: Some("application/x-www-form-urlencoded".to_owned()),
            has_body: true,
            ..Payload::default()
        },
        _ => Payload::default(),
    };
    Ok(Ok(payload))
}

async fn read(
    context: &mut dyn Context,
    object: &Rc<Object>,
    settings: &Settings,
) -> Result<Result<Payload, TransferError>, phera_extension_api::NativeError> {
    let mut bytes = Vec::new();
    let mut remaining = settings.input_size.unwrap_or(u64::MAX);
    while remaining != 0 {
        let limit = remaining.min(64 * 1024);
        let chunk = match (&settings.read, &settings.input) {
            (Some(read), input) => {
                let input = input
                    .as_ref()
                    .map_or(Value::Null, |input| Value::Resource(input.clone()));
                let value = context
                    .call(
                        read,
                        vec![
                            Value::Object(object.clone()),
                            input,
                            Value::Int(limit as i64),
                        ],
                    )
                    .await?;
                match &*value.deref() {
                    Value::String(text) if text.as_bytes().len() as u64 <= limit => {
                        text.as_bytes().to_vec()
                    }
                    Value::Int(268435456) => {
                        return Ok(Err(TransferError::new(42, "operation aborted by callback")));
                    }
                    Value::String(_) => {
                        return Ok(Err(TransferError::new(
                            26,
                            "read function returned too much data",
                        )));
                    }
                    _ => Vec::new(),
                }
            }
            (None, Some(input)) => {
                match phera_ext_streams::native::read_resource(input, limit).await? {
                    Ok(bytes) => bytes,
                    Err(_) => return Ok(Err(TransferError::new(26, "Failed to read upload data"))),
                }
            }
            (None, None) => break,
        };
        if chunk.is_empty() {
            if settings.input_size.is_some() {
                return Ok(Err(TransferError::new(
                    26,
                    format!(
                        "client read function EOF fail, only {}/{} of needed bytes read",
                        bytes.len(),
                        settings.input_size.unwrap_or(0)
                    ),
                )));
            }
            break;
        }
        remaining -= chunk.len() as u64;
        bytes.extend_from_slice(&chunk);
    }
    Ok(Ok(Payload {
        bytes: bytes.into(),
        has_body: true,
        content_type: None,
    }))
}

async fn multipart(
    context: &mut dyn Context,
    fields: &PhpArray,
) -> Result<Result<Payload, TransferError>, phera_extension_api::NativeError> {
    let mut entropy = [0u8; 16];
    if getrandom::fill(&mut entropy).is_err() {
        return Ok(Err(TransferError::new(
            2,
            "Cannot generate multipart boundary",
        )));
    }
    let boundary = format!(
        "------------------------{:032x}",
        u128::from_be_bytes(entropy)
    );
    let mut bytes = Vec::new();
    for (key, value) in fields.iter() {
        let name = match key {
            ArrayKey::String(text) => text.as_bytes().to_vec(),
            ArrayKey::Int(number) => number.to_string().into_bytes(),
        };
        if let Value::Array(items) = &*value.deref() {
            for item in items.values() {
                if let Err(error) = part(context, &mut bytes, &boundary, &name, item).await? {
                    return Ok(Err(error));
                }
            }
        } else if let Err(error) = part(context, &mut bytes, &boundary, &name, value).await? {
            return Ok(Err(error));
        }
    }
    if !fields.is_empty() {
        bytes.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    }
    Ok(Ok(Payload {
        bytes: bytes.into(),
        content_type: Some(format!("multipart/form-data; boundary={boundary}")),
        has_body: true,
    }))
}

async fn part(
    context: &mut dyn Context,
    output: &mut Vec<u8>,
    boundary: &str,
    name: &[u8],
    value: &Value,
) -> Result<Result<(), TransferError>, phera_extension_api::NativeError> {
    output.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"").as_bytes(),
    );
    quoted(output, name);
    output.push(b'"');
    let dereferenced = value.deref();
    if let Value::Object(object) = &*dereferenced
        && object.class.is_instance_of("CURLFile")
    {
        let filename = property(object, "name");
        if filename.contains(&0) {
            return Ok(Err(TransferError::new(
                26,
                "Failed to open/read local data from file/application",
            )));
        }
        let path = match crate::options::path(context, &filename) {
            Ok(path) => path,
            Err(_) => return Ok(Err(TransferError::new(26, "Failed to resolve upload path"))),
        };
        let mut posted = property(object, "postname");
        if posted.is_empty() {
            posted = filename
                .rsplit(|byte| *byte == b'/')
                .next()
                .unwrap_or_default()
                .to_vec();
        }
        let mime = property(object, "mime");
        if let Err(error) = file_header(output, &posted, &mime) {
            return Ok(Err(error));
        }
        let data = match tokio::fs::read(path).await {
            Ok(data) => data,
            Err(_) => {
                return Ok(Err(TransferError::new(
                    26,
                    "Failed to open/read local data from file/application",
                )));
            }
        };
        output.extend_from_slice(&data);
    } else if let Value::Object(object) = &*dereferenced
        && object.class.is_instance_of("CURLStringFile")
    {
        if let Err(error) = file_header(
            output,
            &property(object, "postname"),
            &property(object, "mime"),
        ) {
            return Ok(Err(error));
        }
        output.extend_from_slice(&property(object, "data"));
    } else {
        output.extend_from_slice(b"\r\n\r\n");
        output.extend_from_slice(context.stringify(&dereferenced).await?.as_bytes());
    }
    output.extend_from_slice(b"\r\n");
    Ok(Ok(()))
}

fn property(object: &Object, name: &str) -> Vec<u8> {
    match object.get(&ArrayKey::str(name)) {
        Some(Value::String(text)) => text.as_bytes().to_vec(),
        _ => Vec::new(),
    }
}

fn file_header(output: &mut Vec<u8>, filename: &[u8], mime: &[u8]) -> Result<(), TransferError> {
    if mime.iter().any(|byte| matches!(*byte, b'\r' | b'\n' | 0)) {
        return Err(TransferError::new(43, "Invalid multipart media type"));
    }
    output.extend_from_slice(b"; filename=\"");
    quoted(output, filename);
    output.extend_from_slice(b"\"\r\nContent-Type: ");
    output.extend_from_slice(if mime.is_empty() {
        b"application/octet-stream"
    } else {
        mime
    });
    output.extend_from_slice(b"\r\n\r\n");
    Ok(())
}

fn quoted(output: &mut Vec<u8>, bytes: &[u8]) {
    for byte in bytes {
        match byte {
            b'\r' => output.extend_from_slice(b"%0D"),
            b'\n' => output.extend_from_slice(b"%0A"),
            b'"' => output.extend_from_slice(b"%22"),
            0 => output.extend_from_slice(b"%00"),
            _ => output.push(*byte),
        }
    }
}
