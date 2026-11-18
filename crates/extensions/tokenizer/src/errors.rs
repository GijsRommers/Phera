//! Report parser failures as PHP exceptions. For example,
//! `token_get_all("<?php /* unfinished", TOKEN_PARSE)` throws `ParseError`.

use phera_extension_api::{NativeError, Value};
use phera_lexer::TokenKind;
use phera_parser::ParseError;

use crate::{
    stream::{SourceToken, newlines},
    token,
};

pub(crate) fn exception(class: &'static str, message: impl Into<String>, line: i64) -> NativeError {
    NativeError::throw(class, message)
        .with_property("file", Value::from(""))
        .with_property("line", Value::Int(line))
}

pub(crate) fn translate(
    error: ParseError,
    error_at: Option<usize>,
    tokens: &[SourceToken<'_>],
    code: &[u8],
) -> NativeError {
    let current = error_at.map(|index| &tokens[index]);
    let eof_line = 1 + newlines(code);
    let mut line = current.map_or(eof_line, |token| token.line);
    if error.is_compile_error() {
        return exception("CompileError", error.message, line);
    }
    let message = if error.message.starts_with("invalid numeric literal") {
        "Invalid numeric literal".to_owned()
    } else if let Some(suffix) = error
        .message
        .strip_prefix("invalid UTF-8 codepoint escape sequence")
    {
        format!(
            "Invalid UTF-8 codepoint escape sequence{}",
            suffix.replace("codepoint", "Codepoint"),
        )
    } else if error.message.starts_with("invalid ") && error.message.contains("indentation") {
        if current.is_some_and(|token| token.kind == TokenKind::OpenTag) {
            line = invalid_label_line(tokens).unwrap_or(line);
        }
        error.message.replacen("invalid ", "Invalid ", 1)
    } else if error.message == "unterminated comment" {
        format!("Unterminated comment starting line {line}")
    } else if let Some(message) = literal_error(&error, error_at, tokens, &mut line, eof_line) {
        message
    } else if let Some(message) = unclosed_brace(&error, current, tokens, line) {
        message
    } else {
        syntax_error(&error, error_at, tokens)
    };
    exception("ParseError", message, line)
}

fn invalid_label_line(tokens: &[SourceToken<'_>]) -> Option<i64> {
    let mut starts = Vec::new();
    for token in tokens {
        match token.kind {
            TokenKind::StartHeredoc => starts.push(token.line),
            TokenKind::EndHeredoc => {
                let start = starts.pop()?;
                let indent: Vec<_> = token
                    .text
                    .iter()
                    .copied()
                    .take_while(|byte| matches!(byte, b' ' | b'\t'))
                    .collect();
                if indent.contains(&b' ') && indent.contains(&b'\t') {
                    return Some(start + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn literal_error(
    error: &ParseError,
    index: Option<usize>,
    tokens: &[SourceToken<'_>],
    line: &mut i64,
    eof_line: i64,
) -> Option<String> {
    use TokenKind::*;

    const OFFSET: &str = r#""-" or identifier or variable or number"#;
    const EOF: &str = "syntax error, unexpected end of file";
    let current = index.and_then(|i| tokens.get(i));
    let previous = index
        .unwrap_or(tokens.len())
        .checked_sub(1)
        .and_then(|i| tokens.get(i));

    if let Some(current) = current
        && previous.is_some_and(|token| token.kind == LeftBracket)
    {
        let description = match current.kind {
            StringPart if current.text.is_empty() => Some(r#"string content """#.to_owned()),
            OffsetAmpersand => Some(r#"invalid token "&""#.to_owned()),
            BadCharacter if error.message.starts_with("unexpected character") => {
                Some(format!("character 0x{:02X}", current.text[0]))
            }
            _ => None,
        };
        if let Some(description) = description {
            return Some(format!(
                "syntax error, unexpected {description}, expecting {OFFSET}"
            ));
        }
    }

    let last = tokens.last()?;
    if error.message == "unterminated heredoc" {
        *line = eof_line;
        let expected = r#"variable or heredoc end or "${" or "{$""#;
        return Some(if last.kind == StringPart {
            format!("{EOF}, expecting {expected}")
        } else {
            EOF.to_owned()
        });
    }
    if error.message != "unterminated string" {
        return None;
    }

    if let Some(token) = current
        && token.kind == StringPart
        && (token.text.starts_with(b"'")
            || token.text.starts_with(b"b'")
            || token.text.starts_with(b"B'"))
    {
        let text = description_text(token.text);
        return Some(format!(
            "syntax error, unexpected string content \"{text}\", expecting end of file"
        ));
    }

    if let Some(message) = unclosed_brace(error, None, tokens, eof_line) {
        *line = eof_line;
        return Some(message);
    }
    *line = eof_line;
    let expected = match (current.map(|token| token.kind), last.kind) {
        (Some(Backquote), _) => r#""`""#,
        (Some(DoubleQuote), DoubleQuote) => r#"variable or string content or "${" or "{$""#,
        (Some(DoubleQuote), _) => r#"variable or "${" or "{$""#,
        (_, LeftBracket) => OFFSET,
        (_, NumString) => r#""]""#,
        _ => return Some(EOF.to_owned()),
    };
    Some(format!("{EOF}, expecting {expected}"))
}

fn unclosed_brace(
    error: &ParseError,
    current: Option<&SourceToken<'_>>,
    tokens: &[SourceToken<'_>],
    line: i64,
) -> Option<String> {
    use TokenKind::*;

    let at_end = current.is_none()
        || (error.message.starts_with("expected '}'")
            && current.is_some_and(|token| token.kind == InlineHtml));
    if !at_end {
        return None;
    }
    let mut braces = Vec::new();
    for token in tokens {
        match token.kind {
            LeftBrace | CurlyOpen | DollarCurlyOpen => braces.push(token.line),
            RightBrace => {
                braces.pop();
            }
            _ => {}
        }
    }
    let opening = braces.last()?;
    Some(if *opening == line {
        "Unclosed '{'".to_owned()
    } else {
        format!("Unclosed '{{' on line {opening}")
    })
}

fn syntax_error(error: &ParseError, index: Option<usize>, tokens: &[SourceToken<'_>]) -> String {
    use TokenKind::*;

    let current = index.and_then(|i| tokens.get(i));
    let previous = tokens[..index.unwrap_or(tokens.len())]
        .iter()
        .rev()
        .find(|entry| !token::ignorable(entry.id));
    let found = current.map_or_else(|| "end of file".to_owned(), unexpected_token);
    let mut message = format!("syntax error, unexpected {found}");
    let expected = match (current.map(|t| t.kind), previous.map(|t| t.kind)) {
        (Some(Semicolon), _) if error.message == "expected a variable after '&'" => {
            Some(r#""->" or "?->" or "[""#)
        }
        (Some(LeftBrace), _) if error.message.starts_with("expected '}'") => {
            Some(r#""->" or "?->" or "[""#)
        }
        (Some(LeftBrace), _) if error.message.starts_with("expected ';'") => Some(r#""," or ";""#),
        (Some(_), Some(Function)) if error.message.starts_with("expected an identifier") => {
            Some(r#""(""#)
        }
        (Some(String), Some(Dollar)) => Some(r#"variable or "{" or "$""#),
        (Some(StartHeredoc), _) => None,
        (Some(BadCharacter | Ampersand | AmpersandBeforeVariable), None | Some(Semicolon)) => {
            Some("end of file")
        }
        (None, _) if in_echo_expression(tokens) => Some(r#""," or ";""#),
        (Some(_), Some(Comma))
            if error.message.starts_with("unexpected token")
                && tokens[..index.unwrap()]
                    .iter()
                    .rev()
                    .any(|token| token.kind == LeftBracket) =>
        {
            Some(r#""]""#)
        }
        _ => expected_token(&error.message),
    };
    if let Some(expected) = expected {
        message.push_str(", expecting ");
        message.push_str(expected);
    }
    message
}

fn expected_token(message: &str) -> Option<&'static str> {
    let (expected, _) = message.strip_prefix("expected ")?.split_once(", found ")?;
    match expected {
        "'('" => Some(r#""(""#),
        "')'" => Some(r#"")""#),
        "'{'" => Some(r#""{""#),
        "'}'" => Some(r#""}""#),
        "']'" => Some(r#""]""#),
        "an identifier" => Some("identifier"),
        _ => None,
    }
}

fn in_echo_expression(tokens: &[SourceToken<'_>]) -> bool {
    use TokenKind::*;
    tokens
        .iter()
        .rev()
        .take_while(|t| !matches!(t.kind, Semicolon | CloseTag))
        .any(|token| matches!(token.kind, Echo | OpenTagWithEcho))
}

fn unexpected_token(token: &SourceToken<'_>) -> String {
    use TokenKind::*;

    let text = description_text(token.text);
    let (label, text) = match token.kind {
        Identifier | True | False | Null => ("identifier", text),
        QualifiedName => ("namespaced name", text),
        FullyQualifiedName => ("fully qualified name", text),
        RelativeName => ("namespace-relative name", text),
        Variable => ("variable", text),
        Integer => ("integer", text),
        Float => ("floating-point number", text),
        BadCharacter => return format!("character 0x{:02X}", token.text[0]),
        CloseTag => return r#"token ";""#.to_owned(),
        StartHeredoc => ("heredoc start", text),
        String if token.text.starts_with(b"\"") => ("double-quoted string", text),
        String if token.text.starts_with(b"'") => ("single-quoted string", text),
        String => ("quoted string", text),
        _ => ("token", text.to_ascii_lowercase()),
    };
    format!("{label} \"{text}\"")
}

fn description_text(bytes: &[u8]) -> String {
    let content = bytes.split(|&byte| byte == b'\n').next().unwrap_or(bytes);
    let content = content
        .strip_prefix(b"\"")
        .or_else(|| content.strip_prefix(b"'"))
        .unwrap_or(content);
    let content = content
        .strip_suffix(b"\"")
        .or_else(|| content.strip_suffix(b"'"))
        .unwrap_or(content);
    let (content, suffix) = if content.len() > 33 {
        (&content[..30], "...")
    } else {
        (content, "")
    };
    let printable = content.split(|&byte| byte == 0).next().unwrap_or(content);
    format!("{}{suffix}", String::from_utf8_lossy(printable))
}
