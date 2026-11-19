//! Lexical diagnostics for code such as `echo "\400";` or `(integer) $value`.

use phera_source::{CompileDiagnostic, CompileDiagnosticKind, LineTable};

use super::{Mode, ScannerError, Token, TokenKind, nesting::Nesting};

/// Raw tokenization decodes quoted strings, while parsed PHP also decodes heredocs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScanMode {
    Raw,
    Parsed,
}

#[derive(Debug, Default)]
pub struct ScanReport {
    pub diagnostics: Vec<(usize, CompileDiagnostic)>,
    pub nesting_error: Option<ScannerError>,
}

pub(super) struct ScannerDiagnostics {
    scan: ScanMode,
    nesting: Nesting,
    lines: LineTable,
    entries: Vec<(usize, CompileDiagnostic)>,
}

impl ScannerDiagnostics {
    pub(super) fn new(source: &[u8], scan: ScanMode) -> Self {
        Self {
            scan,
            nesting: Nesting::default(),
            lines: LineTable::from_bytes(source),
            entries: Vec::new(),
        }
    }

    pub(super) fn finish(self) -> ScanReport {
        ScanReport {
            diagnostics: self.entries,
            nesting_error: self.nesting.into_error(),
        }
    }

    pub(super) fn inspect(&mut self, mode: Mode, token: &Token, source: &[u8]) {
        use TokenKind::*;

        if self.scan == ScanMode::Parsed {
            self.nesting.inspect(mode, token, &self.lines);
        }

        let text = &source[token.span.start..token.span.end];
        match token.kind {
            Backquote if self.scan == ScanMode::Parsed && mode == Mode::Php => {
                self.push(
                    token.span.start,
                    CompileDiagnosticKind::Deprecated,
                    "The backtick (`) operator is deprecated, use shell_exec() instead".into(),
                );
            }
            String => {
                let quote = usize::from(matches!(text.first(), Some(b'b' | b'B')));
                if text[quote] == b'"' {
                    self.escape_warnings(
                        &text[quote + 1..text.len() - 1],
                        token.span.start + quote + 1,
                    );
                }
            }
            StringPart
                if matches!(mode, Mode::DoubleQuotes | Mode::Backquotes)
                    || (self.scan == ScanMode::Parsed && matches!(mode, Mode::Heredoc { .. })) =>
            {
                self.escape_warnings(text, token.span.start);
            }
            IntCast | BoolCast | FloatCast | StringCast if self.scan == ScanMode::Parsed => {
                let inner = text[1..text.len() - 1].trim_ascii();
                let (old, canonical) = if inner.eq_ignore_ascii_case(b"integer") {
                    ("integer", "int")
                } else if inner.eq_ignore_ascii_case(b"double") {
                    ("double", "float")
                } else if inner.eq_ignore_ascii_case(b"boolean") {
                    ("boolean", "bool")
                } else if inner.eq_ignore_ascii_case(b"binary") {
                    ("binary", "string")
                } else {
                    return;
                };
                self.push(
                    token.span.start,
                    CompileDiagnosticKind::Deprecated,
                    format!(
                        "Non-canonical cast ({old}) is deprecated, use the ({canonical}) cast instead"
                    ),
                );
            }
            _ => {}
        }
    }

    /// Zend's heredoc lookahead suppresses this warning; scan each escape once.
    fn escape_warnings(&mut self, bytes: &[u8], start: usize) {
        let mut position = 0;
        while let Some(relative) = bytes[position..].iter().position(|&byte| byte == b'\\') {
            let escape = position + relative;
            let rest = &bytes[escape + 1..];
            match rest {
                [a @ b'4'..=b'7', b @ b'0'..=b'7', c @ b'0'..=b'7', ..] => {
                    let (a, b, c) = (char::from(*a), char::from(*b), char::from(*c));
                    self.push(
                        start + escape,
                        CompileDiagnosticKind::Warning,
                        format!(r"Octal escape sequence overflow \{a}{b}{c} is greater than \377"),
                    );
                }
                [b'u', b'{', digits @ ..] if !valid_codepoint(digits) => return,
                [] => return,
                _ => {}
            }
            position = escape + 2;
        }
    }

    fn push(&mut self, offset: usize, kind: CompileDiagnosticKind, message: String) {
        self.entries.push((
            offset,
            CompileDiagnostic {
                kind,
                message,
                line: self.lines.line(offset),
            },
        ));
    }
}

fn valid_codepoint(bytes: &[u8]) -> bool {
    let Some(end) = bytes.iter().position(|byte| !byte.is_ascii_hexdigit()) else {
        return false;
    };
    if end == 0 || bytes[end] != b'}' {
        return false;
    }
    let digits = bytes[..end]
        .iter()
        .copied()
        .skip_while(|byte| *byte == b'0');
    let mut value = 0;
    for digit in digits {
        value = value * 16 + char::from(digit).to_digit(16).expect("hex digit");
        if value > 0x10ffff {
            return false;
        }
    }
    true
}
