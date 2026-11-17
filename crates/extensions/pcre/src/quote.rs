//! `preg_quote('a.b#c', '/')` escapes punctuation and represents NUL as `\000`.

use phera_extension_api::{Context, NativeResult, Value, args};
pub(crate) fn quote(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let delimiter = args::bytes(arguments, 1).first().copied();
    let mut result = Vec::new();
    for &byte in args::bytes(arguments, 0) {
        if byte == 0 {
            result.extend_from_slice(b"\\000");
            continue;
        }
        if b".\\+*?[^]$(){}=!<>|:-#".contains(&byte) || Some(byte) == delimiter {
            result.push(b'\\');
        }
        result.push(byte);
    }
    Ok(Value::from(result))
}
