//! `get_meta_tags('page.html')` reads named meta elements before the end of the head.

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
    registry.function(FunctionEntry::asynchronous(
        "get_meta_tags",
        const {
            &[
                FILENAME,
                Param::new("use_include_path", ParamType::Bool)
                    .with_default(ConstantValue::Bool(false)),
            ]
        },
        read,
    ));
    registry.describe_function("get_meta_tags", "array|false", Some("standard"));
}

fn read(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        const FUNCTION: &str = "get_meta_tags";
        let name = args::bytes(&arguments, 0);
        check_path(FUNCTION, "#1 ($filename)", name)?;
        let include_path = args::bool(&arguments, 1).unwrap_or(false);
        let stream =
            open_resource(context, FUNCTION, name, b"rb", include_path, &Value::Null).await?;
        match filtered_files::contents(context, stream, FUNCTION, 0, None).await? {
            Value::String(source) => Ok(Value::from(meta_tags(source.as_bytes()))),
            other => Ok(other),
        }
    })
}

fn meta_tags(mut source: &[u8]) -> PhpArray {
    let mut values = PhpArray::new();
    while let Some(tag_start) = source.iter().position(|&byte| byte == b'<') {
        source = &source[tag_start + 1..];
        let Some((end, complete)) = tag_end(source) else {
            break;
        };
        if !complete {
            source = &source[end..];
            continue;
        }
        let element = &source[..end];
        source = &source[end + 1..];
        let mut split = element.splitn(2, |byte: &u8| byte.is_ascii_whitespace());
        let name = split.next().unwrap_or_default();
        if name.eq_ignore_ascii_case(b"/head") {
            break;
        }
        if name.eq_ignore_ascii_case(b"meta") {
            let attributes = split.next().unwrap_or_default();
            if let Some((key, content)) = named_meta(attributes) {
                values.insert(ArrayKey::String(key.into()), Value::from(content.to_vec()));
            }
        }
    }
    values
}

fn tag_end(source: &[u8]) -> Option<(usize, bool)> {
    let mut quote = None;
    for (index, &byte) in source.iter().enumerate() {
        if let Some(quoted) = quote {
            if byte == quoted {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'>' => return Some((index, true)),
                b'<' => return Some((index, false)),
                _ => {}
            }
        }
    }
    None
}

fn named_meta(mut attrs: &[u8]) -> Option<(Vec<u8>, &[u8])> {
    let mut name = None;
    let mut content = None;
    loop {
        attrs = attrs.trim_ascii_start();
        let key_len = attrs
            .iter()
            .position(|byte| byte.is_ascii_whitespace() || *byte == b'=')
            .unwrap_or(attrs.len());
        if key_len == 0 {
            break;
        }
        let key = &attrs[..key_len];
        attrs = attrs[key_len..].trim_ascii_start();
        let Some(rest) = attrs.strip_prefix(b"=") else {
            continue;
        };
        let (value, rest) = attr_value(rest.trim_ascii_start())?;
        if key.eq_ignore_ascii_case(b"name") {
            name = Some(value);
        } else if key.eq_ignore_ascii_case(b"content") {
            content = Some(value);
        }
        attrs = rest;
    }
    let name = name?;
    let key = name
        .iter()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                byte.to_ascii_lowercase()
            } else {
                b'_'
            }
        })
        .collect();
    Some((key, content.unwrap_or_default()))
}

fn attr_value(source: &[u8]) -> Option<(&[u8], &[u8])> {
    if let Some((&quote, rest)) = source.split_first()
        && matches!(quote, b'\'' | b'"')
    {
        let end = rest.iter().position(|&byte| byte == quote)?;
        Some((&rest[..end], &rest[end + 1..]))
    } else {
        let end = source
            .iter()
            .position(u8::is_ascii_whitespace)
            .unwrap_or(source.len());
        Some((&source[..end], &source[end..]))
    }
}
