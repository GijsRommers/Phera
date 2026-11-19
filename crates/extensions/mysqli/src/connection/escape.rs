//! `mysqli_real_escape_string($db, $text)` follows the active SQL escaping mode.

use super::Handle;
use phera_extension_api::{NativeError, NativeResult, Value};

pub fn escape(handle: Handle, bytes: &[u8]) -> NativeResult {
    let state = handle.borrow();
    state.lease()?;
    let charset = state.charset.as_deref().unwrap_or("utf8mb4");
    if matches!(
        charset,
        "gb18030" | "gb2312" | "gbk" | "big5" | "sjis" | "cp932"
    ) {
        return Err(NativeError::error(format!(
            "mysqli escaping for character set {charset} is not supported"
        )));
    }
    let mut output = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        if !state.backslashes {
            if byte == b'\'' {
                output.push(b'\'');
            }
            output.push(byte);
            continue;
        }
        match byte {
            0 => output.extend_from_slice(b"\\0"),
            b'\n' => output.extend_from_slice(b"\\n"),
            b'\r' => output.extend_from_slice(b"\\r"),
            26 => output.extend_from_slice(b"\\Z"),
            b'\\' | b'\'' | b'"' => output.extend_from_slice(&[b'\\', byte]),
            _ => output.push(byte),
        }
    }
    Ok(Value::from(output))
}
