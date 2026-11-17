//! Iconv MIME fields include the field name: `iconv_mime_encode('Subject', 'hello')`.
use crate::{iconv, mime};
use phera_extension_api::{ArrayKey, Context, NativeResult, PhpArray, Value, args};
pub fn encode(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let options = args::array(a, 2);
    let get = |key: &str| options.and_then(|a| a.get(&ArrayKey::str(key)));
    let input = get("input-charset")
        .map(crate::string_of)
        .unwrap_or_else(|| iconv::encoding(ctx, &[], 0));
    let output = get("output-charset")
        .map(crate::string_of)
        .unwrap_or_else(|| input.clone());
    let method = get("scheme")
        .map(crate::string_of)
        .unwrap_or_else(|| "B".into());
    let method = mime::Scheme::parse(method.as_bytes());
    let line_length = get("line-length")
        .and_then(|v| {
            if let Value::Int(n) = v {
                Some(*n)
            } else {
                None
            }
        })
        .unwrap_or(76)
        .max(0) as usize;
    let newline = get("line-break-chars")
        .map(crate::string_of)
        .unwrap_or_else(|| "\r\n".into());
    let Some(converted) =
        iconv::convert_bytes(ctx, args::bytes(a, 1), &input, &output, "iconv_mime_encode")
    else {
        return Ok(Value::Bool(false));
    };
    let mut result = args::bytes(a, 0).to_vec();
    result.extend_from_slice(b": ");
    let overhead = output.len() + 7 + result.len();
    if overhead >= line_length {
        ctx.warning("iconv_mime_encode(): Buffer length exceeded".into());
        return Ok(Value::Bool(false));
    }
    let budget = line_length - overhead;
    let mut chunk = Vec::new();
    for byte in converted {
        let mut next = chunk.clone();
        next.push(byte);
        if !chunk.is_empty()
            && (method.encode(&next).len() > budget
                || matches!(method, mime::Scheme::Base64)
                    && next.len() > (budget / 4 * 3).saturating_sub(4))
        {
            result.extend_from_slice(method.word(&chunk, &output).as_bytes());
            result.extend_from_slice(newline.as_bytes());
            result.push(b' ');
            chunk.clear();
        }
        chunk.push(byte);
    }
    result.extend_from_slice(method.word(&chunk, &output).as_bytes());
    Ok(Value::from(result))
}
pub fn decode(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let target = iconv::encoding(ctx, a, 2);
    Ok(crate::iconv_decode::decode(
        ctx,
        args::bytes(a, 0),
        &target,
        &crate::iconv_decode::Mode::new(args::int(a, 1).unwrap_or(0)),
        "iconv_mime_decode",
    )
    .map_or(Value::Bool(false), Value::from))
}

pub fn headers(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let mut lines: Vec<Vec<u8>> = Vec::new();
    for line in args::bytes(a, 0).split(|b| *b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            break;
        }
        if matches!(line.first(), Some(b' ' | b'\t')) {
            if let Some(last) = lines.last_mut() {
                last.extend_from_slice(line);
            }
        } else {
            lines.push(line.to_vec());
        }
    }
    let mut result = PhpArray::new();
    for line in lines {
        let target = iconv::encoding(ctx, a, 2);
        let Some(_) = crate::iconv_decode::decode(
            ctx,
            &line,
            &target,
            &crate::iconv_decode::Mode::new(args::int(a, 1).unwrap_or(0)),
            "iconv_mime_decode_headers",
        ) else {
            return Ok(Value::Bool(false));
        };
        let Some(colon) = line.iter().position(|b| *b == b':') else {
            continue;
        };
        let key = ArrayKey::str(&line[..colon]);
        let mut value = &line[colon + 1..];
        while matches!(value.first(), Some(b' ' | b'\t')) {
            value = &value[1..];
        }
        let value = decode(
            ctx,
            &[
                Value::from(value),
                args::value(a, 1).clone(),
                args::value(a, 2).clone(),
            ],
        )?;
        if let Some(previous) = result.get(&key).cloned() {
            let mut values = match previous {
                Value::Array(a) => (*a).clone(),
                other => {
                    let mut array = PhpArray::new();
                    array.push(other)?;
                    array
                }
            };
            values.push(value)?;
            result.insert(key, Value::from(values));
        } else {
            result.insert(key, value);
        }
    }
    Ok(Value::from(result))
}
