//! Multipart headers and in-memory fields are buffered. Files are opened by the body.

use phera_extension_api::{ArrayKey, Context, Object, PhpArray, Value};

use crate::error::TransferError;

use super::{
    Content, Payload,
    frames::{self, Segment},
};

pub(super) async fn build(
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
    let mut segments = Vec::new();
    for (key, value) in fields.iter() {
        let name = match key {
            ArrayKey::Interned(text) => text.as_bytes().to_vec(),
            ArrayKey::String(text) => text.as_bytes().to_vec(),
            ArrayKey::Int(number) => number.to_string().into_bytes(),
        };
        if let Value::Array(items) = &*value.deref() {
            for item in items.values() {
                if let Err(error) =
                    part(context, (&mut bytes, &mut segments), &boundary, &name, item).await?
                {
                    return Ok(Err(error));
                }
            }
        } else if let Err(error) = part(
            context,
            (&mut bytes, &mut segments),
            &boundary,
            &name,
            value,
        )
        .await?
        {
            return Ok(Err(error));
        }
    }
    if !fields.is_empty() {
        bytes.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    }
    if !bytes.is_empty() {
        segments.push(Segment::Bytes(bytes.into()));
    }
    let length = segments
        .iter()
        .try_fold(0u64, |total, segment| total.checked_add(segment.length()));
    let Some(length) = length else {
        return Ok(Err(TransferError::new(26, "Multipart upload is too large")));
    };
    Ok(Ok(Payload {
        content: Content::Files(segments.into()),
        length: Some(length),
        content_type: Some(format!("multipart/form-data; boundary={boundary}")),
        has_body: true,
    }))
}

async fn part(
    context: &mut dyn Context,
    (output, segments): (&mut Vec<u8>, &mut Vec<Segment>),
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
        let Ok(metadata) = tokio::fs::metadata(&path).await else {
            return Ok(Err(frames::file_error()));
        };
        if !metadata.is_file() {
            return Ok(Err(frames::file_error()));
        }
        segments.push(Segment::Bytes(std::mem::take(output).into()));
        segments.push(Segment::File {
            path,
            length: metadata.len(),
        });
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
    object
        .get(&ArrayKey::str(name))
        .and_then(|v| v.as_str_bytes().map(|b| b.to_vec()))
        .unwrap_or_default()
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
