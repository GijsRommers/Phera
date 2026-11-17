//! Parse form fields: `mb_parse_str('a[]=1&a[]=2', $result)` creates an array.
use phera_extension_api::{ArrayKey, Context, NativeResult, PhpArray, Value, args};
use std::rc::Rc;
pub fn parse(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    for (i, pair) in args::bytes(a, 0).split(|b| *b == b'&').enumerate() {
        if i >= 1000 {
            ctx.warning(
                "mb_parse_str(): Input variables exceeded 1000. \
                To increase the limit change max_input_vars in php.ini."
                    .into(),
            );
            break;
        }
        let position = pair.iter().position(|b| *b == b'=').unwrap_or(pair.len());
        let key = decode(&pair[..position]);
        let key = key.split(|b| *b == 0).next().unwrap_or_default();
        let value = decode(pair.get(position + 1..).unwrap_or_default());
        let state = ctx
            .state()
            .get_or_default::<crate::state::Settings>()
            .clone();
        let value = crate::codec::encode(
            &crate::codec::decode(&value, state.internal),
            state.internal,
            &state.substitute,
        );
        let bracket = key.iter().position(|b| *b == b'[').unwrap_or(key.len());
        let base: Vec<u8> = key[..bracket]
            .iter()
            .map(|b| if matches!(b, b' ' | b'.') { b'_' } else { *b })
            .collect();
        if base.is_empty() {
            continue;
        }
        let mut path = vec![Some(ArrayKey::str(base))];
        let mut rest = &key[bracket..];
        while rest.first() == Some(&b'[') {
            let Some(close) = rest.iter().position(|b| *b == b']') else {
                break;
            };
            path.push(if close == 1 {
                None
            } else {
                Some(ArrayKey::str(&rest[1..close]))
            });
            rest = &rest[close + 1..];
        }
        insert(&mut result, path, Value::from(value));
    }
    if let Some(reference) = args::reference(a, 1) {
        *reference.borrow_mut() = Value::from(result);
    }
    Ok(Value::Bool(!args::bytes(a, 0).is_empty()))
}
fn insert(root: &mut PhpArray, path: Vec<Option<ArrayKey>>, value: Value) {
    let mut parents = Vec::new();
    let mut current = std::mem::take(root);
    let last = path.len() - 1;
    for (i, key) in path.into_iter().enumerate() {
        if i == last {
            if let Some(key) = key {
                current.insert(key, value.clone());
            } else {
                let _ = current.push(value.clone());
            }
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
        let value = Value::Array(Rc::new(current));
        if let Some(key) = key {
            parent.insert(key, value);
        } else {
            let _ = parent.push(value);
        }
        current = parent;
    }
    *root = current;
}
pub fn decode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let digits = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(n) = digits.and_then(|s| u8::from_str_radix(s, 16).ok()) {
                out.push(n);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    out
}
