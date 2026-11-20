//! Build a second PCRE2 pattern that records the marks a match reached.
//!
//! ```php
//! preg_match('/(?|(a)(*:first)|(b)(*:second))/', 'b', $m);
//! // $m = [0 => 'b', 1 => 'b', 'MARK' => 'second']
//! ```

use crate::pattern::Options;
use crate::transform::{self, Mode, Source};

pub(crate) struct Probe {
    pub body: String,
    pub groups: usize,
    /// Capture number and original name, in order of occurrence in the pattern.
    pub marks: Vec<(usize, Vec<u8>)>,
}

/// Keep the verbs themselves, so that named SKIP and backtracking still
/// see them. Remove branch resets so each probe has a distinct number.
pub(crate) fn insert(
    source: &Source,
    startup: usize,
    original: &[u8],
    options: &Options,
) -> Option<Probe> {
    let body = &source.text[startup..];
    if !body.contains("(*") {
        return None;
    }
    let mut probe = Probe {
        body: String::new(),
        groups: 0,
        marks: Vec::new(),
    };
    let mut mode = Mode::new(options);
    let mut scopes = Vec::new();
    let mut cursor = 0;
    let mut copied = 0;
    let mut numeric_reference = false;
    let newline = Newline::from_startup(&source.text[..startup]);

    while cursor < body.len() {
        let tail = &body.as_bytes()[cursor..];
        match tail {
            [b'\\', b'Q', ..] => cursor += quote_end(tail),
            [b'\\', escaped, rest @ ..] => {
                numeric_reference |=
                    matches!(escaped, b'1'..=b'9') || (*escaped == b'g' && numeric(rest));
                cursor += if *escaped == b'c' { 3 } else { 2 };
            }
            [b'[', ..] => cursor += class_end(tail),
            [b'#', ..] if mode.extended => cursor += newline.comment_end(tail),
            [b'(', b'?', b'#', ..] => cursor += through(tail, b')'),
            [b'(', b'*', ..] => {
                let (verb, _) = body[cursor + 2..].split_once(')')?;
                let end = cursor + verb.len() + 3;
                if let Some((kind, _)) = verb.split_once(':') {
                    if matches!(kind, "" | "MARK" | "PRUNE" | "THEN" | "COMMIT") {
                        probe.body.push_str(&body[copied..end]);
                        probe.body.push_str(if mode.no_auto_capture {
                            "(?-n:())"
                        } else {
                            "()"
                        });
                        probe.groups += 1;
                        let name_start = source.offsets[startup + cursor + 3 + kind.len()];
                        let name_end = source.offsets[startup + end - 1];
                        probe
                            .marks
                            .push((probe.groups, original[name_start..name_end].into()));
                        copied = end;
                    } else if is_group_verb(kind) {
                        // These verbs open groups and their bodies are pattern syntax.
                        scopes.push(mode);
                        cursor += kind.len() + 3;
                        continue;
                    }
                }
                cursor = end;
            }
            [b'(', b'?', b'|', ..] => {
                scopes.push(mode);
                probe.body.push_str(&body[copied..cursor]);
                probe.body.push_str("(?:");
                cursor += 3;
                copied = cursor;
            }
            [b'(', b'?', b'(', condition @ ..] => {
                scopes.push(mode);
                if condition.starts_with(b"?") {
                    // An assertion condition is itself a group, with its own body.
                    cursor += 2;
                } else {
                    numeric_reference |=
                        numeric(condition) || condition.strip_prefix(b"R").is_some_and(numeric);
                    cursor += 3 + through(condition, b')');
                }
            }
            [b'(', b'?', b'C', ..] => cursor += callout_end(&tail[3..]) + 3,
            [b'(', b'?', b'[', ..] => return None,
            [b'(', ..] => {
                if let Some(inline) = transform::inline_options(tail, mode) {
                    if inline.scoped {
                        scopes.push(mode);
                    }
                    mode = inline.mode;
                    cursor += inline.length;
                    continue;
                }
                scopes.push(mode);
                if let Some(special) = tail.strip_prefix(b"(?") {
                    numeric_reference |= numeric(special);
                    if named_capture(special) {
                        probe.groups += 1;
                    }
                } else if !mode.no_auto_capture {
                    probe.groups += 1;
                }
                cursor += 1;
            }
            [b')', ..] => {
                if let Some(outer) = scopes.pop() {
                    mode = outer;
                }
                cursor += 1;
            }
            _ => cursor += 1,
        }
    }
    // Inserting groups can renumber references even without a branch reset.
    if probe.marks.is_empty() || numeric_reference {
        return None;
    }
    probe.body.push_str(&body[copied..]);
    Some(probe)
}

fn numeric(tail: &[u8]) -> bool {
    let tail = match tail {
        [b'{' | b'<' | b'\'', rest @ ..] => rest,
        _ => tail,
    };
    let tail = match tail {
        [b'+' | b'-', rest @ ..] => rest,
        _ => tail,
    };
    tail.first().is_some_and(u8::is_ascii_digit)
}

fn named_capture(tail: &[u8]) -> bool {
    matches!(tail, [b'<', name, ..] if !matches!(name, b'=' | b'!' | b'*'))
        || tail.starts_with(b"P<")
        || tail.starts_with(b"'")
}

fn is_group_verb(kind: &str) -> bool {
    matches!(
        kind,
        "atomic"
            | "pla"
            | "positive_lookahead"
            | "nla"
            | "negative_lookahead"
            | "plb"
            | "positive_lookbehind"
            | "nlb"
            | "negative_lookbehind"
            | "napla"
            | "non_atomic_positive_lookahead"
            | "naplb"
            | "non_atomic_positive_lookbehind"
            | "sr"
            | "script_run"
            | "asr"
            | "atomic_script_run"
    )
}

/// Bytes through the first delimiter, or all remaining bytes.
fn through(tail: &[u8], end: u8) -> usize {
    tail.iter()
        .position(|&byte| byte == end)
        .map_or(tail.len(), |i| i + 1)
}

fn quote_end(tail: &[u8]) -> usize {
    tail[2..]
        .windows(2)
        .position(|pair| pair == b"\\E")
        .map_or(tail.len(), |i| i + 4)
}

fn class_end(tail: &[u8]) -> usize {
    let mut cursor = 1;
    if tail.get(cursor) == Some(&b'^') {
        cursor += 1;
    }
    if tail.get(cursor) == Some(&b']') {
        cursor += 1;
    }
    while cursor < tail.len() {
        match &tail[cursor..] {
            [b']', ..] => return cursor + 1,
            [b'\\', b'Q', ..] => cursor += quote_end(&tail[cursor..]),
            [b'\\', b'c', _, ..] => cursor += 3,
            [b'\\', _, ..] => cursor += 2,
            [b'[', kind @ (b':' | b'.' | b'='), rest @ ..] => {
                cursor += rest
                    .windows(2)
                    .position(|pair| pair == [*kind, b']'])
                    .map_or(1, |i| i + 4);
            }
            _ => cursor += 1,
        }
    }
    cursor
}

fn callout_end(tail: &[u8]) -> usize {
    let Some(&open) = tail.first() else {
        return 0;
    };
    if open.is_ascii_digit() || open == b')' {
        return through(tail, b')');
    }
    let close = if open == b'{' { b'}' } else { open };
    let mut cursor = 1;
    while cursor < tail.len() {
        if tail[cursor] == close {
            if tail.get(cursor + 1) != Some(&close) {
                return cursor + 1 + through(&tail[cursor + 1..], b')');
            }
            cursor += 1;
        }
        cursor += 1;
    }
    cursor
}

#[derive(Clone, Copy)]
enum Newline {
    Byte(u8),
    CrLf,
    AnyCrLf,
    Any,
}

impl Newline {
    fn from_startup(startup: &str) -> Self {
        let mut newline = Self::Byte(b'\n');
        for directive in startup.split("(*") {
            newline = match directive.trim_end_matches(')') {
                "CR" => Self::Byte(b'\r'),
                "LF" => Self::Byte(b'\n'),
                "NUL" => Self::Byte(b'\0'),
                "CRLF" => Self::CrLf,
                "ANYCRLF" => Self::AnyCrLf,
                "ANY" => Self::Any,
                _ => continue,
            };
        }
        newline
    }

    fn comment_end(self, tail: &[u8]) -> usize {
        tail.iter()
            .enumerate()
            .find_map(|(i, &byte)| match self {
                Self::Byte(end) if end == byte => Some(i + 1),
                Self::CrLf if tail[i..].starts_with(b"\r\n") => Some(i + 2),
                Self::Any | Self::AnyCrLf if matches!(byte, b'\r' | b'\n') => Some(i + 1),
                Self::Any if matches!(byte, 0x0b | 0x0c | 0x85) => Some(i + 1),
                Self::Any if tail[i..].starts_with(b"\xc2\x85") => Some(i + 2),
                Self::Any if matches!(&tail[i..], [0xe2, 0x80, 0xa8 | 0xa9, ..]) => Some(i + 3),
                _ => None,
            })
            .unwrap_or(tail.len())
    }
}
