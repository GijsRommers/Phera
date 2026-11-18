//! Parsing a query string into an array, as `parse_str` does, with each
//! value checked against the internal encoding:
//!
//! ```php
//! mb_parse_str('a[]=1&a[]=2&b.c=%C3%A9', $result);
//! // $result === ['a' => ['1', '2'], 'b_c' => 'é']
//! ```

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Registry, Value,
    args,
};

use crate::codec::{decode, encode, hex_byte};
use crate::state::Settings;

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "mb_parse_str",
        MB_PARSE_STR,
        mb_parse_str,
    ));
}

/// PHP's default `max_input_vars`.
const MAX_INPUT_VARS: usize = 1000;

const MB_PARSE_STR: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("result", ParamType::Mixed).nullable().by_ref(),
];

/// `mb_parse_str(string $string, array &$result): bool`
fn mb_parse_str(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let settings = ctx.state().get_or_default::<Settings>().clone();
    let input = args::bytes(a, 0);
    let mut result = PhpArray::new();
    for (index, pair) in input.split(|&byte| byte == b'&').enumerate() {
        if index >= MAX_INPUT_VARS {
            ctx.warning(
                "mb_parse_str(): Input variables exceeded 1000. \
                To increase the limit change max_input_vars in php.ini."
                    .into(),
            );
            break;
        }
        let equals = pair
            .iter()
            .position(|&byte| byte == b'=')
            .unwrap_or(pair.len());
        let key = url_decode(&pair[..equals]);
        // A NUL ends the name, as in C.
        let key = key.split(|&byte| byte == 0).next().unwrap_or_default();
        let value = url_decode(pair.get(equals + 1..).unwrap_or_default());
        // Invalid sequences in the value become the substitute character.
        let value = encode(
            &decode(&value, settings.internal),
            settings.internal,
            &settings.substitute,
        );
        let Some(path) = key_path(key) else {
            continue;
        };
        insert(&mut result, path, Value::from(value));
    }
    if let Some(reference) = args::reference(a, 1) {
        *reference.borrow_mut() = Value::from(result);
    }
    Ok(Value::Bool(!input.is_empty()))
}

/// `a[b][]` as the keys `a`, `b` and an append; `None` for an empty name.
/// In the name before the first bracket, PHP replaces spaces and dots with
/// underscores.
fn key_path(key: &[u8]) -> Option<Vec<Option<ArrayKey>>> {
    let bracket = key
        .iter()
        .position(|&byte| byte == b'[')
        .unwrap_or(key.len());
    let base: Vec<u8> = key[..bracket]
        .iter()
        .map(|&byte| {
            if matches!(byte, b' ' | b'.') {
                b'_'
            } else {
                byte
            }
        })
        .collect();
    if base.is_empty() {
        return None;
    }
    let mut path = vec![Some(ArrayKey::str(base))];
    let mut rest = &key[bracket..];
    while rest.first() == Some(&b'[') {
        let Some(close) = rest.iter().position(|&byte| byte == b']') else {
            break;
        };
        path.push((close > 1).then(|| ArrayKey::str(&rest[1..close])));
        rest = &rest[close + 1..];
    }
    Some(path)
}

/// Stores `value` under the path of keys, `None` meaning `[]`. The arrays
/// along the path are taken out, changed and put back in, without
/// recursion.
fn insert(root: &mut PhpArray, path: Vec<Option<ArrayKey>>, value: Value) {
    let mut parents = Vec::new();
    let mut current = std::mem::take(root);
    let last = path.len() - 1;
    for (index, key) in path.into_iter().enumerate() {
        if index == last {
            store(&mut current, key, value);
            break;
        }
        let next = key.as_ref().and_then(|key| current.get(key)).cloned();
        parents.push((current, key));
        current = match next {
            Some(Value::Array(array)) => (*array).clone(),
            _ => PhpArray::new(),
        };
    }
    while let Some((mut parent, key)) = parents.pop() {
        store(&mut parent, key, Value::Array(Rc::new(current)));
        current = parent;
    }
    *root = current;
}

fn store(array: &mut PhpArray, key: Option<ArrayKey>, value: Value) {
    match key {
        Some(key) => {
            array.insert(key, value);
        }
        // Appending fails after the key PHP_INT_MAX
        // (`a[9223372036854775807]=1&a[]=2`); PHP drops the value silently too.
        None => {
            let _ = array.push(value);
        }
    }
}

/// `%XX` escapes and `+` for a space.
fn url_decode(bytes: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Some(byte) = hex_byte(&bytes[index + 1..index + 3])
        {
            decoded.push(byte);
            index += 3;
            continue;
        }
        decoded.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    decoded
}
