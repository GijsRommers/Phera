//! PHP scanner errors for mismatched delimiters, such as `echo (1];`.

use phera_source::{LineTable, Span};

use super::{Mode, Token, TokenKind};

#[derive(Debug)]
pub struct ScannerError {
    pub message: String,
    pub span: Span,
    pub line: usize,
}

#[derive(Default)]
pub(super) struct Nesting {
    open: Vec<(char, usize)>,
    error: Option<ScannerError>,
}

impl Nesting {
    pub(super) fn inspect(&mut self, mode: Mode, token: &Token, lines: &LineTable) {
        use TokenKind::*;

        if self.error.is_some() || matches!(mode, Mode::VarOffset | Mode::Halted) {
            return;
        }
        if let Some(delimiter) = match token.kind {
            LeftParen => Some('('),
            LeftBracket | AttributeStart => Some('['),
            LeftBrace | CurlyOpen | DollarCurlyOpen => Some('{'),
            _ => None,
        } {
            self.open.push((delimiter, lines.line(token.span.start)));
            return;
        }

        let closing = match token.kind {
            RightParen => Some(')'),
            RightBracket => Some(']'),
            RightBrace => Some('}'),
            Eof if !self.open.is_empty() => None,
            _ => return,
        };
        let line = lines.line(token.span.start);
        let message = match self.open.pop() {
            Some((open, _))
                if matches!(
                    (open, closing),
                    ('(', Some(')')) | ('[', Some(']')) | ('{', Some('}'))
                ) =>
            {
                return;
            }
            Some((delimiter, opened)) => {
                let mut message = format!("Unclosed '{delimiter}'");
                if opened != line {
                    message.push_str(&format!(" on line {opened}"));
                }
                if let Some(closing) = closing {
                    message.push_str(&format!(" does not match '{closing}'"));
                }
                message
            }
            None if let Some(closing) = closing => format!("Unmatched '{closing}'"),
            None => return,
        };
        self.error = Some(ScannerError {
            message,
            span: token.span,
            line,
        });
    }

    pub(super) fn into_error(self) -> Option<ScannerError> {
        self.error
    }
}
