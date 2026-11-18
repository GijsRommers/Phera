//! `html_entity_decode("&copy;")` returns `"©"`.
//!
//! Decoding replaces only complete, recognized references. All other bytes
//! pass through, including invalid encoded bytes and disallowed characters.

use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray,
    PhpString, Value, args,
};

use super::{
    DEFAULT_FLAGS, Encoding, Options, buffer, entities, numeric_entity, options::Document,
};
use entities::Entity;

/// `html_entity_decode('&copy; &#x1F680;', ENT_HTML5)` returns `"© 🚀"`.
pub(super) const HTML_ENTITY_DECODE: FunctionEntry = FunctionEntry::sync(
    "html_entity_decode",
    &[
        Param::new("string", ParamType::String),
        Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(DEFAULT_FLAGS)),
        Param::new("encoding", ParamType::String)
            .nullable()
            .optional(),
    ],
    html_entity_decode,
);

fn html_entity_decode(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 0);
    if !input.contains(&b'&') {
        return Ok(arguments[0].clone());
    }
    let options = Options::from_flags(args::int(arguments, 1).unwrap_or(DEFAULT_FLAGS));
    let encoding = Encoding::resolve(context, "html_entity_decode", args::bytes(arguments, 2));
    let mut output = buffer(input.len().saturating_add(input.len() / 5))?;
    let mut remaining = input;
    while let Some(start) = remaining.iter().position(|&byte| byte == b'&') {
        output.extend_from_slice(&remaining[..start]);
        let tail = &remaining[start + 1..];
        if let Some((entity, end)) = reference(tail, options.document)
            && includes(entity, options)
            && (entity.second.is_none() || matches!(encoding, Encoding::Utf8))
            && encoding.encode_entity(entity.first, &mut output).is_some()
        {
            if let Some(second) = entity.second {
                encoding.encode_entity(second, &mut output);
            }
            remaining = &tail[end + 1..];
        } else {
            output.push(b'&');
            remaining = tail;
        }
    }
    output.extend_from_slice(remaining);
    Ok(Value::from(output))
}

fn reference(tail: &[u8], document: Document) -> Option<(Entity, usize)> {
    if let Some(digits) = tail.strip_prefix(b"#") {
        let end = numeric_end(digits)? + 1;
        let first = match &tail[..end] {
            [b'#', b'x' | b'X', b'0', b'x' | b'X', rest @ ..] => {
                super::parse_numeric_digits(rest, 16)?
            }
            name => numeric_entity(name)?,
        };
        if !document.allows(first) || (document == Document::Html5 && first == 0x0d) {
            return None;
        }
        return Some((
            Entity {
                name: "",
                first,
                second: None,
            },
            end,
        ));
    }

    // PHP's longest name is 31 ASCII characters, excluding '&' and ';'.
    let end = tail.iter().take(32).position(|&byte| byte == b';')?;
    let entity = entities::named(&tail[..end], document)?;
    Some((entity, end))
}

fn numeric_end(input: &[u8]) -> Option<usize> {
    let (digits, prefix) = match input {
        [b'x' | b'X', b'0', b'x' | b'X', digits @ ..] => (digits, 3),
        [b'x' | b'X', digits @ ..] => (digits, 1),
        digits => (digits, 0),
    };
    let end = if prefix == 0 {
        digits.iter().position(|byte| !byte.is_ascii_digit())?
    } else {
        digits.iter().position(|byte| !byte.is_ascii_hexdigit())?
    };
    (digits[end] == b';' && end > 0).then_some(prefix + end)
}

fn includes(entity: Entity, options: Options) -> bool {
    u8::try_from(entity.first).is_err() || options.quotes.includes(entity.first as u8)
}

/// `get_html_translation_table()[">"]` returns `"&gt;"`.
pub(super) const GET_HTML_TRANSLATION_TABLE: FunctionEntry = FunctionEntry::sync(
    "get_html_translation_table",
    &[
        Param::new("table", ParamType::Int).with_default(ConstantValue::Int(0)),
        Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(DEFAULT_FLAGS)),
        Param::new("encoding", ParamType::String).optional(),
    ],
    get_html_translation_table,
);

fn get_html_translation_table(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let options = Options::from_flags(args::int(arguments, 1).unwrap_or(DEFAULT_FLAGS));
    let encoding = Encoding::resolve(
        context,
        "get_html_translation_table",
        args::bytes(arguments, 2),
    );
    let all = args::int(arguments, 0).unwrap_or(0) != 0
        && options.document != Document::Xml1
        && matches!(
            encoding,
            Encoding::Utf8 | Encoding::Latin1 | Encoding::SingleByte(_)
        );
    let rows = entities::translation(options.document, all);
    let mut table = PhpArray::new();
    let mut key = Vec::with_capacity(8);
    if let (true, Encoding::SingleByte(legacy)) = (all, &encoding) {
        // PHP orders non-Latin-1 single-byte tables by encoded byte value.
        for byte in 0..=u8::MAX {
            let point = legacy.to_unicode(byte);
            let start = rows.partition_point(|row| row.first < point);
            for &row in rows[start..].iter().take_while(|row| row.first == point) {
                if includes(row, options) {
                    key.clear();
                    key.push(byte);
                    if row
                        .second
                        .is_none_or(|point| encoding.encode_entity(point, &mut key).is_some())
                    {
                        insert(&mut table, &key, row.name);
                    }
                }
            }
        }
    } else {
        for &row in rows.iter().filter(|&&row| includes(row, options)) {
            key.clear();
            if !all {
                key.push(row.first as u8);
            } else if encoding.encode_entity(row.first, &mut key).is_none() {
                continue;
            }
            if let Some(second) = row.second {
                if matches!(encoding, Encoding::Latin1) {
                    // PHP writes a paired character's low byte for Latin-1.
                    key.push(second as u8);
                } else {
                    encoding.encode_entity(second, &mut key);
                }
            }
            insert(&mut table, &key, row.name);
        }
    }
    Ok(Value::from(table))
}

fn insert(table: &mut PhpArray, key: &[u8], name: &str) {
    table.insert(
        ArrayKey::str(PhpString::from(key)),
        Value::from(format!("&{name};")),
    );
}
