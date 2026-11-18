//! `eval('return [1,')` reports its unclosed delimiter at the evaluated source location.

use phera_lexer::{Lexer, TokenKind};
use phera_source::LineTable;

pub(crate) fn message(
    message: String,
    source: &str,
    token: &str,
    offset: usize,
    line: usize,
    lines: &LineTable,
) -> String {
    let at_end = message.contains("end of file");
    if (at_end || matches!(token, ")" | "]" | "}"))
        && let Some(problem) = delimiter_error(source, offset, at_end, lines)
    {
        return problem.display(line);
    }
    if message.starts_with("syntax error") || message.starts_with("Invalid ") {
        return message;
    }
    if at_end {
        let expectation = if message.starts_with("expected ';'") {
            if source
                .trim_start_matches("<?php ")
                .trim_start()
                .starts_with("echo ")
            {
                ", expecting \",\" or \";\""
            } else {
                ", expecting \";\""
            }
        } else {
            ""
        };
        return format!("syntax error, unexpected end of file{expectation}");
    }
    if token == "<" && message.starts_with("unexpected") {
        return "syntax error, unexpected token \"<\", expecting end of file".into();
    }
    if message.starts_with("unexpected character")
        && let [byte] = token.as_bytes()
    {
        return format!("syntax error, unexpected character 0x{byte:02X}");
    }
    if message.starts_with("unexpected") {
        return format!("syntax error, {message}");
    }
    message
}

enum DelimiterError {
    Unmatched(char),
    Unclosed {
        opening: char,
        line: usize,
        closing: Option<char>,
    },
}

impl DelimiterError {
    fn display(self, reported_line: usize) -> String {
        match self {
            Self::Unmatched(closing) => format!("Unmatched '{closing}'"),
            Self::Unclosed {
                opening,
                line,
                closing,
            } => {
                let mut message = format!("Unclosed '{opening}'");
                if line != reported_line {
                    message.push_str(&format!(" on line {line}"));
                }
                if let Some(closing) = closing {
                    message.push_str(&format!(" does not match '{closing}'"));
                }
                message
            }
        }
    }
}

/// Lexical tokens keep delimiters in comments, strings and HTML out of the stack.
fn delimiter_error(
    source: &str,
    offset: usize,
    at_end: bool,
    lines: &LineTable,
) -> Option<DelimiterError> {
    use TokenKind::*;
    let mut stack = Vec::new();
    let mut lexer = Lexer::new(source);
    while let Ok(token) = lexer.next_token() {
        if !at_end && token.span.start > offset {
            break;
        }
        let (closing, expected) = match token.kind {
            LeftParen | LeftBracket | AttributeStart | LeftBrace | CurlyOpen => {
                let opening = match token.kind {
                    LeftParen => '(',
                    LeftBracket | AttributeStart => '[',
                    _ => '{',
                };
                stack.push((opening, lines.line(token.span.start)));
                continue;
            }
            RightParen => (')', '('),
            RightBracket => (']', '['),
            RightBrace => ('}', '{'),
            Eof => break,
            _ => continue,
        };
        let Some((opening, line)) = stack.pop() else {
            return Some(DelimiterError::Unmatched(closing));
        };
        if opening != expected {
            return Some(DelimiterError::Unclosed {
                opening,
                line,
                closing: Some(closing),
            });
        }
    }
    if at_end {
        stack.pop().map(|(opening, line)| DelimiterError::Unclosed {
            opening,
            line,
            closing: None,
        })
    } else {
        None
    }
}
