//! PHP `/end$/D` requires the actual end; `/a/A` begins at the supplied offset.

use crate::pattern::Options;

pub(crate) struct Source {
    pub text: String,
    pub offsets: Vec<usize>,
    pub closing: &'static str,
}
impl Source {
    fn rewritten(&mut self, text: &str, offset: usize) {
        self.text.push_str(text);
        self.offsets.extend(std::iter::repeat_n(offset, text.len()));
    }
    fn original(&mut self, bytes: &[u8], offset: usize) {
        let text = std::str::from_utf8(bytes).expect("ASCII or checked UTF-8");
        self.text.push_str(text);
        self.offsets.extend(offset..offset + bytes.len());
    }
}

#[derive(Clone, Copy)]
struct Mode {
    multiline: bool,
    extended: bool,
}

/// Track local option scopes so `/a$(?m:b$)/D` only rewrites the first dollar.
fn inline_options(tail: &[u8], current: Mode) -> Option<(Mode, usize, bool)> {
    let tail = tail.strip_prefix(b"(?")?;
    let mut mode = current;
    let mut enabled = true;
    for (index, &byte) in tail.iter().enumerate() {
        match byte {
            b'^' => {
                mode = Mode {
                    multiline: false,
                    extended: false,
                };
            }
            b'-' => enabled = false,
            b'm' => mode.multiline = enabled,
            b'x' => mode.extended = enabled,
            b'i' | b's' | b'n' | b'r' | b'J' | b'U' | b'a' | b'T' | b'P' | b'S' | b'W' => {}
            b':' | b')' if index > 0 => return Some((mode, index + 3, byte == b':')),
            _ => return None,
        }
    }
    None
}

/// Keep a source map because PCRE2 reports byte offsets in the rewritten pattern.
pub(crate) fn source(body: &[u8], options: &Options) -> Result<Source, String> {
    let mut source = Source {
        text: String::new(),
        offsets: Vec::new(),
        closing: "",
    };
    let mut mode = Mode {
        multiline: options.multiline,
        extended: options.extended,
    };
    let mut scopes = Vec::new();
    let mut class = false;
    let mut class_initial = false;
    let mut quoted = false;
    let mut cursor = 0;
    while cursor < body.len() {
        let byte = body[cursor];
        let tail = &body[cursor..];
        if !quoted && !class && (tail.starts_with(b"(?#") || (mode.extended && byte == b'#')) {
            let ending = if byte == b'#' { b'\n' } else { b')' };
            let length = tail
                .iter()
                .position(|&byte| byte == ending)
                .map_or(tail.len(), |index| index + 1);
            // Comments can contain non-UTF bytes. Encode those as literal bytes represented
            // by characters; PCRE2 ignores the comment contents either way.
            let comment = String::from_utf8_lossy(&tail[..length]);
            source.rewritten(&comment, cursor);
            if byte == b'#' && !tail[..length].ends_with(b"\n") {
                source.closing = "\n";
            }
            cursor += length;
            continue;
        }
        if byte == b'\\' && cursor + 1 < body.len() {
            let escaped = body[cursor + 1];
            let token = !quoted || escaped == b'E';
            if !quoted && !class && escaped == b'C' && options.utf {
                return Err(format!(
                    "Compilation failed: using \\C is incompatible with the 'u' modifier \
                     at offset {}",
                    cursor + 2,
                ));
            }
            if escaped == b'Q' && !quoted {
                quoted = true;
            } else if escaped == b'E' {
                quoted = false;
            }
            if escaped.is_ascii() && token {
                source.original(&body[cursor..cursor + 2], cursor);
                cursor += 2;
                class_initial = false;
                continue;
            }
        }
        if !quoted {
            if class {
                if byte == b']' && !class_initial {
                    class = false;
                }
                if byte != b'^' {
                    class_initial = false;
                }
                if tail.starts_with(b"[:") || tail.starts_with(b"[.") || tail.starts_with(b"[=") {
                    let end = [tail[1], b']'];
                    if let Some(length) = tail.windows(2).position(|pair| pair == end) {
                        let token = &tail[..length + 2];
                        if token.is_ascii() {
                            source.original(token, cursor);
                            cursor += token.len();
                            continue;
                        }
                    }
                }
            } else {
                match byte {
                    b'[' => {
                        class = true;
                        class_initial = true;
                    }
                    b'(' => {
                        if let Some((updated, length, scoped)) = inline_options(tail, mode) {
                            if scoped {
                                scopes.push(mode);
                            }
                            mode = updated;
                            source.original(&tail[..length], cursor);
                            cursor += length;
                            continue;
                        }
                        scopes.push(mode);
                    }
                    b')' => {
                        if let Some(outer) = scopes.pop() {
                            mode = outer;
                        }
                    }
                    b'$' if options.dollar_end && !mode.multiline => {
                        source.rewritten(r"\z", cursor);
                        cursor += 1;
                        continue;
                    }
                    _ => {}
                }
            }
        }
        if byte.is_ascii() {
            source.original(&body[cursor..cursor + 1], cursor);
            cursor += 1;
        } else {
            // Decode one character rather than requiring the entire suffix to be UTF-8.
            let length = match byte {
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => 1,
            };
            let character = &body[cursor..(cursor + length).min(body.len())];
            if std::str::from_utf8(character).is_ok() {
                source.original(character, cursor);
                cursor += character.len();
            } else {
                // The safe compiler requires &str; escapes preserve arbitrary byte patterns.
                if quoted {
                    source.rewritten(r"\E", cursor);
                }
                source.rewritten(&format!("\\x{byte:02x}"), cursor);
                if quoted {
                    source.rewritten(r"\Q", cursor);
                }
                cursor += 1;
            }
        }
    }
    if quoted {
        source.closing = r"\E";
    }
    source.offsets.push(body.len());
    Ok(source)
}

/// Startup directives must stay ahead of inline options and the anchor wrapper.
pub(crate) fn startup_end(body: &str) -> usize {
    let mut position = 0;
    while let Some(tail) = body
        .get(position..)
        .and_then(|tail| tail.strip_prefix("(*"))
    {
        let Some(end) = tail.find(')') else {
            break;
        };
        let directive = &tail[..end];
        if !matches!(
            directive,
            "UTF"
                | "UTF8"
                | "UCP"
                | "CR"
                | "LF"
                | "CRLF"
                | "ANY"
                | "ANYCRLF"
                | "NUL"
                | "BSR_ANYCRLF"
                | "BSR_UNICODE"
                | "NO_JIT"
                | "NO_AUTO_POSSESS"
                | "NO_START_OPT"
                | "NO_DOTSTAR_ANCHOR"
                | "NOTEMPTY"
                | "NOTEMPTY_ATSTART"
        ) && !directive.starts_with("LIMIT_")
        {
            break;
        }
        position += end + 3;
    }
    position
}
