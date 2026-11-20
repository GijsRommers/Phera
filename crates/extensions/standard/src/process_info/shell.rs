//! `escapeshellcmd('echo a;b')` escapes the command's shell metacharacters.
//! Unlike `escapeshellarg()`, balanced quotes and spaces keep their meaning.

use nix::unistd::{SysconfVar, sysconf};
use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

pub(super) fn escape(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let command = args::bytes(arguments, 0);
    if command.contains(&0) {
        return Err(NativeError::value_error(
            "escapeshellcmd(): Argument #1 ($command) must not contain any null bytes",
        ));
    }
    Ok(Value::from(escape_shell_command(command)?))
}

/// Unix mail and escapeshellcmd use the same quoting rules.
pub fn escape_shell_command(command: &[u8]) -> Result<Vec<u8>, NativeError> {
    let limit = sysconf(SysconfVar::ARG_MAX)
        .ok()
        .flatten()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(4096);
    if command.len() > limit.saturating_sub(3) {
        return Err(NativeError::value_error(format!(
            "Command exceeds the allowed length of {limit} bytes"
        )));
    }
    let utf8 = ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .filter_map(std::env::var_os)
        .find(|value| !value.is_empty())
        .is_some_and(|locale| {
            let locale = locale.to_string_lossy().to_ascii_lowercase();
            locale.contains("utf-8") || locale.contains("utf8")
        });
    let mut output = Vec::with_capacity(command.len());
    let mut quotes = None;
    // Walk ASCII quotes once from right to left to identify the final quote of
    // each kind. PHP pairs quotes without interpreting shell grammar.
    let last_single = command.iter().rposition(|&byte| byte == b'\'');
    let last_double = command.iter().rposition(|&byte| byte == b'"');
    let mut offset = 0;
    for chunk in command.utf8_chunks() {
        for character in chunk.valid().chars() {
            let position = offset;
            offset += character.len_utf8();
            if !character.is_ascii() {
                if utf8 {
                    output.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
                }
                continue;
            }
            let byte = character as u8;
            let escape = match byte {
                b'\'' | b'"' => {
                    let last = if byte == b'\'' {
                        last_single
                    } else {
                        last_double
                    };
                    match quotes {
                        Some(open) if open == byte => {
                            quotes = None;
                            false
                        }
                        None if last.is_some_and(|last| last > position) => {
                            quotes = Some(byte);
                            false
                        }
                        _ => true,
                    }
                }
                _ => b"#&;`|*?~<>^()[]{}$\\\n".contains(&byte),
            };
            if escape {
                output.push(b'\\');
            }
            output.push(byte);
        }
        offset += chunk.invalid().len();
    }
    if output.len() > limit.saturating_add(1) {
        return Err(NativeError::value_error(format!(
            "Escaped command exceeds the allowed length of {limit} bytes"
        )));
    }
    Ok(output)
}
