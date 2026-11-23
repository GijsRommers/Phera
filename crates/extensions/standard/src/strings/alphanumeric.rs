//! `str_increment('a9')` gives 'b0' and `str_decrement('b0')` gives 'a9'.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::sync("str_rot13", PARAMS, rot13),
    FunctionEntry::sync("str_increment", PARAMS, increment),
    FunctionEntry::sync("str_decrement", PARAMS, decrement),
];

const PARAMS: &[Param] = &[Param::new("string", ParamType::String)];

fn rot13(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let source = args::bytes(arguments, 0);
    let mut output = super::buffer(source.len())?;
    output.extend(source.iter().map(|byte| match byte {
        b'a'..=b'm' | b'A'..=b'M' => byte + 13,
        b'n'..=b'z' | b'N'..=b'Z' => byte - 13,
        _ => *byte,
    }));
    Ok(Value::from(output))
}

fn increment(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let source = checked("str_increment", arguments)?;
    let mut output = super::buffer(source.len().saturating_add(1))?;
    output.extend_from_slice(source);
    for byte in output.iter_mut().rev() {
        let (first, last) = alphabet(*byte);
        if *byte != last {
            *byte += 1;
            return Ok(Value::from(output));
        }
        *byte = first;
    }
    let (first, _) = alphabet(source[0]);
    output.insert(0, if first == b'0' { b'1' } else { first });
    Ok(Value::from(output))
}

fn decrement(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let source = checked("str_decrement", arguments)?;
    if source[0] == b'0' {
        return Err(out_of_range(source));
    }
    let mut output = super::buffer(source.len())?;
    output.extend_from_slice(source);
    for (position, byte) in output.iter_mut().enumerate().rev() {
        let (first, last) = alphabet(*byte);
        if *byte != first {
            *byte -= 1;
            if position == 0 && *byte == b'0' && source.len() > 1 {
                output.remove(0);
            }
            return Ok(Value::from(output));
        }
        *byte = last;
    }
    if source.len() == 1 {
        return Err(out_of_range(source));
    }
    output.remove(0);
    Ok(Value::from(output))
}

fn alphabet(byte: u8) -> (u8, u8) {
    match byte {
        b'0'..=b'9' => (b'0', b'9'),
        b'A'..=b'Z' => (b'A', b'Z'),
        _ => (b'a', b'z'),
    }
}

fn checked<'a>(function: &str, arguments: &'a [Value]) -> Result<&'a [u8], NativeError> {
    let source = args::bytes(arguments, 0);
    if source.is_empty() {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($string) must not be empty"
        )));
    }
    if !source.iter().all(u8::is_ascii_alphanumeric) {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($string) must be composed only of alphanumeric ASCII characters"
        )));
    }
    Ok(source)
}

fn out_of_range(source: &[u8]) -> NativeError {
    NativeError::value_error(format!(
        "str_decrement(): Argument #1 ($string) \"{}\" is out of decrement range",
        String::from_utf8_lossy(source)
    ))
}
