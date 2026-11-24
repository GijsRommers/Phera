//! Binary codec for per-connection WebSocket context bags stored in Rust.
//!
//! Only `null`, `bool`, `int`, `float`, `string`, and nested arrays of those
//! values are allowed; objects, closures, resources, and circular references
//! are rejected with `TypeError`. Total encoded bytes are capped by INI
//! `phera.ws_max_context_bytes` (default 16 KiB).

use std::rc::Rc;

use phera_extension_api::{Context, NativeError, ini};
use phera_value::{ArrayKey, PhpArray, Value};

pub(crate) const DEFAULT_MAX_CONTEXT_BYTES: usize = 16 * 1024;
const INI_MAX_CONTEXT_BYTES: &[u8] = b"phera.ws_max_context_bytes";
const MAX_DEPTH: usize = 64;

const TAG_NULL: u8 = 0;
const TAG_FALSE: u8 = 1;
const TAG_TRUE: u8 = 2;
const TAG_INT: u8 = 3;
const TAG_FLOAT: u8 = 4;
const TAG_STRING: u8 = 5;
const TAG_ARRAY: u8 = 6;

const KEY_INT: u8 = 0;
const KEY_STRING: u8 = 1;

/// Returns the configured maximum encoded byte size for one connection context.
pub(crate) fn max_context_bytes(context: &dyn Context) -> usize {
    context
        .ini_get(INI_MAX_CONTEXT_BYTES)
        .map(|bytes| ini::integer(bytes.as_bytes()).max(0) as usize)
        .unwrap_or(DEFAULT_MAX_CONTEXT_BYTES)
}

/// Validates and serializes a context value according to `phera.ws_max_context_bytes`.
pub(crate) fn encode_context(context: &dyn Context, value: &Value) -> Result<Vec<u8>, NativeError> {
    let limit = max_context_bytes(context);
    let mut out = Vec::new();
    let mut active_arrays = Vec::new();
    encode_value(value, &mut out, &mut active_arrays, limit)?;
    Ok(out)
}

/// Serializes an initial `PhpArray` passed to `Phera\WebSocket\accept()`.
pub(crate) fn encode_context_array(
    context: &dyn Context,
    array: &PhpArray,
) -> Result<Vec<u8>, NativeError> {
    let limit = max_context_bytes(context);
    let mut out = Vec::new();
    let mut active_arrays = Vec::new();
    encode_array(array, None, &mut out, &mut active_arrays, limit)?;
    Ok(out)
}

/// Decodes context bytes stored in `WebSocketHub` back into a PHP `Value`.
pub(crate) fn decode_context(bytes: &[u8]) -> Value {
    if bytes.is_empty() {
        return Value::from(PhpArray::new());
    }
    let mut cursor = bytes;
    decode_value(&mut cursor).unwrap_or_else(|| Value::from(PhpArray::new()))
}

fn check_limit(len: usize, limit: usize) -> Result<(), NativeError> {
    if len > limit {
        return Err(NativeError::throw(
            "ValueError",
            format!("WebSocket context exceeds phera.ws_max_context_bytes ({limit} bytes)"),
        ));
    }
    Ok(())
}

fn encode_value(
    value: &Value,
    out: &mut Vec<u8>,
    active_arrays: &mut Vec<usize>,
    limit: usize,
) -> Result<(), NativeError> {
    match &*value.deref() {
        Value::Null => out.push(TAG_NULL),
        Value::Bool(false) => out.push(TAG_FALSE),
        Value::Bool(true) => out.push(TAG_TRUE),
        Value::Int(number) => {
            out.push(TAG_INT);
            out.extend_from_slice(&number.to_le_bytes());
        }
        Value::Float(number) => {
            out.push(TAG_FLOAT);
            out.extend_from_slice(&number.to_bits().to_le_bytes());
        }
        Value::InternedStr(text) => {
            encode_bytes_value(text.as_bytes(), out, limit)?;
        }
        Value::String(text) => {
            encode_bytes_value(text.as_bytes(), out, limit)?;
        }
        Value::Array(array) => {
            encode_array(
                array,
                Some(Rc::as_ptr(array) as usize),
                out,
                active_arrays,
                limit,
            )?;
        }
        Value::Object(_) | Value::Resource(_) | Value::Reference(_) => {
            return Err(NativeError::throw(
                "TypeError",
                "WebSocket connection context may only contain null, bool, int, float, string, or arrays of those values",
            ));
        }
    }
    check_limit(out.len(), limit)
}

fn encode_bytes_value(bytes: &[u8], out: &mut Vec<u8>, limit: usize) -> Result<(), NativeError> {
    let Ok(len) = u32::try_from(bytes.len()) else {
        return Err(NativeError::throw(
            "ValueError",
            "WebSocket context string too large",
        ));
    };
    out.push(TAG_STRING);
    out.extend_from_slice(&len.to_le_bytes());
    check_limit(out.len().saturating_add(bytes.len()), limit)?;
    out.extend_from_slice(bytes);
    Ok(())
}

fn encode_array(
    array: &PhpArray,
    ptr: Option<usize>,
    out: &mut Vec<u8>,
    active_arrays: &mut Vec<usize>,
    limit: usize,
) -> Result<(), NativeError> {
    if active_arrays.len() >= MAX_DEPTH {
        return Err(NativeError::throw(
            "ValueError",
            "WebSocket connection context exceeds maximum nesting depth",
        ));
    }
    if let Some(addr) = ptr {
        if active_arrays.contains(&addr) {
            return Err(NativeError::throw(
                "TypeError",
                "Circular reference is not allowed in WebSocket connection context",
            ));
        }
        active_arrays.push(addr);
    }

    let result = (|| -> Result<(), NativeError> {
        let Ok(count) = u32::try_from(array.len()) else {
            return Err(NativeError::throw(
                "ValueError",
                "WebSocket context array too large",
            ));
        };
        out.push(TAG_ARRAY);
        out.extend_from_slice(&count.to_le_bytes());
        check_limit(out.len(), limit)?;

        for (key, item) in array.iter() {
            match key {
                ArrayKey::Int(number) => {
                    out.push(KEY_INT);
                    out.extend_from_slice(&number.to_le_bytes());
                }
                ArrayKey::Interned(name) => {
                    encode_key_bytes(name.as_bytes(), out, limit)?;
                }
                ArrayKey::String(name) => {
                    encode_key_bytes(name.as_bytes(), out, limit)?;
                }
            }
            check_limit(out.len(), limit)?;
            encode_value(item, out, active_arrays, limit)?;
        }
        Ok(())
    })();

    if ptr.is_some() {
        active_arrays.pop();
    }
    result
}

fn encode_key_bytes(bytes: &[u8], out: &mut Vec<u8>, limit: usize) -> Result<(), NativeError> {
    let Ok(len) = u32::try_from(bytes.len()) else {
        return Err(NativeError::throw(
            "ValueError",
            "WebSocket context key too large",
        ));
    };
    out.push(KEY_STRING);
    out.extend_from_slice(&len.to_le_bytes());
    check_limit(out.len().saturating_add(bytes.len()), limit)?;
    out.extend_from_slice(bytes);
    Ok(())
}

fn decode_value(cursor: &mut &[u8]) -> Option<Value> {
    let tag = read_byte(cursor)?;
    match tag {
        TAG_NULL => Some(Value::Null),
        TAG_FALSE => Some(Value::Bool(false)),
        TAG_TRUE => Some(Value::Bool(true)),
        TAG_INT => Some(Value::Int(read_i64(cursor)?)),
        TAG_FLOAT => Some(Value::Float(f64::from_bits(read_u64(cursor)?))),
        TAG_STRING => {
            let len = read_u32(cursor)? as usize;
            Some(Value::from(read_slice(cursor, len)?))
        }
        TAG_ARRAY => {
            let count = read_u32(cursor)? as usize;
            let mut array = PhpArray::with_capacity(count.min(1024));
            for _ in 0..count {
                let key = match read_byte(cursor)? {
                    KEY_INT => ArrayKey::Int(read_i64(cursor)?),
                    KEY_STRING => {
                        let len = read_u32(cursor)? as usize;
                        ArrayKey::str(read_slice(cursor, len)?.to_vec())
                    }
                    _ => return None,
                };
                let value = decode_value(cursor)?;
                array.insert(key, value);
            }
            Some(Value::from(array))
        }
        _ => None,
    }
}

fn read_byte(cursor: &mut &[u8]) -> Option<u8> {
    let (&first, rest) = cursor.split_first()?;
    *cursor = rest;
    Some(first)
}

fn read_slice<'a>(cursor: &mut &'a [u8], len: usize) -> Option<&'a [u8]> {
    let (head, rest) = cursor.split_at_checked(len)?;
    *cursor = rest;
    Some(head)
}

fn read_u32(cursor: &mut &[u8]) -> Option<u32> {
    let bytes: [u8; 4] = read_slice(cursor, 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn read_u64(cursor: &mut &[u8]) -> Option<u64> {
    let bytes: [u8; 8] = read_slice(cursor, 8)?.try_into().ok()?;
    Some(u64::from_le_bytes(bytes))
}

fn read_i64(cursor: &mut &[u8]) -> Option<i64> {
    let bytes: [u8; 8] = read_slice(cursor, 8)?.try_into().ok()?;
    Some(i64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use phera_extension_api::testing::TestContext;
    use phera_value::{ArrayKey, ClassHandle, Object, PhpArray, Value};

    use super::{decode_context, encode_context};

    struct DummyClass;
    impl ClassHandle for DummyClass {
        fn name(&self) -> &str {
            "stdClass"
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[test]
    fn roundtrips_scalars_and_nested_arrays() {
        let context = TestContext::default();
        let mut inner = PhpArray::new();
        inner.push(Value::Int(1)).expect("push");
        inner.push(Value::Float(2.5)).expect("push");
        inner.push(Value::Bool(true)).expect("push");
        inner.push(Value::Null).expect("push");

        let mut root = PhpArray::new();
        root.insert(ArrayKey::str("user_id"), Value::Int(42));
        root.insert(ArrayKey::str("name"), Value::from("Alice"));
        root.insert(ArrayKey::str("tags"), Value::from(inner));

        let encoded = encode_context(&context, &Value::from(root)).expect("valid context");
        let decoded = decode_context(&encoded);
        let Value::Array(arr) = decoded else {
            panic!("expected array");
        };
        assert_eq!(arr.get(&ArrayKey::str("user_id")), Some(&Value::Int(42)));
        assert_eq!(
            arr.get(&ArrayKey::str("name")).map(|v| v.as_str_bytes()),
            Some(Some(&b"Alice"[..]))
        );
    }

    #[test]
    fn rejects_objects_and_circular_arrays_with_type_error() {
        let context = TestContext::default();
        let obj = Rc::new(Object::new(1, Rc::new(DummyClass)));
        let err = encode_context(&context, &Value::Object(obj)).unwrap_err();
        assert!(format!("{err:?}").contains("TypeError"));

        let shared = Rc::new(RefCell::new(Value::Null));
        let mut arr = PhpArray::new();
        arr.insert(ArrayKey::str("self"), Value::Reference(shared.clone()));
        *shared.borrow_mut() = Value::from(arr.clone());
        let cyc_err = encode_context(&context, &Value::Reference(shared)).unwrap_err();
        assert!(format!("{cyc_err:?}").contains("TypeError"));
    }

    #[test]
    fn rejects_oversized_context_with_value_error() {
        let context = TestContext::default();
        let huge = Value::from(vec![b'a'; 17_000]);
        let err = encode_context(&context, &huge).unwrap_err();
        assert!(format!("{err:?}").contains("ValueError"));
    }
}
