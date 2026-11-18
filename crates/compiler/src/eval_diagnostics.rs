//! `eval('return [1,')` reports its unclosed delimiter at the evaluated source location.

use phera_lexer::{Lexer, TokenKind};
use phera_source::LineTable;

pub(crate) fn message(
    message: String,
    source: &str,
    token: &str,
    line: usize,
    lines: &LineTable,
) -> String {
    if message.starts_with("syntax error") || message.starts_with("Invalid ") {
        return message;
    }
    if message.contains("end of file") {
        if let Some((delimiter, opened)) = last_open_delimiter(source, lines) {
            return if opened == line {
                format!("Unclosed '{delimiter}'")
            } else {
                format!("Unclosed '{delimiter}' on line {opened}")
            };
        }
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
    if matches!(token, ")" | "]" | "}") && message.starts_with("unexpected") {
        return format!("Unmatched '{token}'");
    }
    if token == "<" && message.starts_with("unexpected") {
        return "syntax error, unexpected token \"<\", expecting end of file".into();
    }
    if message.starts_with("unexpected") {
        return format!("syntax error, {message}");
    }
    message
}

/// Lexical tokens keep parentheses in comments, strings and HTML out of the stack.
fn last_open_delimiter(source: &str, lines: &LineTable) -> Option<(char, usize)> {
    use TokenKind::*;
    let mut stack = Vec::new();
    let mut lexer = Lexer::new(source);
    while let Ok(token) = lexer.next_token() {
        let opening = match token.kind {
            LeftParen => '(',
            LeftBracket | AttributeStart => '[',
            LeftBrace => '{',
            RightParen | RightBracket | RightBrace => {
                stack.pop();
                continue;
            }
            Eof => break,
            _ => continue,
        };
        stack.push((opening, lines.line(token.span.start)));
    }
    stack.pop()
}
