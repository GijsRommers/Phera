//! PHP diagnostic wording shared by the runtime and lint.
//! For example, `echo ;` reports an unexpected semicolon.

use crate::ParseError;
use phera_source::LineTable;

pub(crate) fn php_diagnostic(
    error: &ParseError,
    source: &[u8],
    lines: &LineTable,
) -> (usize, String) {
    let original = lines.line(error.span.start);
    if error.is_compile_error() || error.is_backtick_operator() {
        return (original, error.message.clone());
    }
    let token = source
        .get(error.span.start..error.span.end)
        .unwrap_or_default();
    let source = String::from_utf8_lossy(source);
    let token = String::from_utf8_lossy(token);
    let line = parse_line(&error.message, &source, original);
    let message = parse_message(&error.message, &source, &token, line);
    (line, message)
}

/// Preserve frontend errors while normalizing the CLI wording PHP exposes.
fn parse_message(message: &str, source: &str, token: &str, line: usize) -> String {
    if message == "unterminated comment" {
        return format!("Unterminated comment starting line {line}");
    }
    if message.starts_with("invalid numeric literal") {
        return "Invalid numeric literal".to_owned();
    }
    if message == "unsupported syntax: the (real) cast, removed in PHP 8" {
        return "The (real) cast has been removed, use (float) instead".to_owned();
    }
    if let Some(rest) = message.strip_prefix("invalid ") {
        return format!(
            "Invalid {}",
            rest.replace("codepoint too large", "Codepoint too large")
        );
    }
    if message == "unterminated heredoc" {
        let suffix = if source.ends_with("\n\n") {
            concat!(
                ", expecting variable or heredoc end or \"$",
                "{\" or \"{$\""
            )
        } else {
            ""
        };
        return format!("syntax error, unexpected end of file{suffix}");
    }
    let Some((expected, found)) = message
        .strip_prefix("expected ")
        .and_then(|message| message.split_once(", found token "))
    else {
        return message.to_owned();
    };
    if expected == "a variable" && token.starts_with('"') {
        return format!(
            "syntax error, unexpected double-quoted string {token}, expecting variable or \"{{\" or \"$\""
        );
    }
    if expected == "'}'" && source.contains("__halt_compiler") {
        let line = source
            .split('{')
            .next()
            .map_or(1, |prefix| prefix.lines().count());
        return format!("Unclosed '{{' on line {line}");
    }
    let raw = found.trim_matches('"');
    let unexpected = if found.starts_with("\"<<<") {
        "token \"<<\"".to_owned()
    } else if !raw.is_empty() && raw.bytes().all(|byte| byte.is_ascii_digit()) {
        format!("integer {found}")
    } else if !raw.is_empty()
        && raw
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        format!("identifier {found}")
    } else {
        format!("token {found}")
    };
    let expectation = match expected {
        "')'" => ", expecting \")\"",
        "'class'" => ", expecting \"class\"",
        "'}'" if found == "\"{\"" => ", expecting \"->\" or \"?->\" or \"[\"",
        "';' or close tag" if unexpected.starts_with("identifier") => "",
        "';' or close tag" if source.contains("const ") => ", expecting \",\" or \";\"",
        "';' or close tag" => ", expecting \";\"",
        _ => return message.to_owned(),
    };
    format!("syntax error, unexpected {unexpected}{expectation}")
}

/// PHP points at the offending body line, rather than the whole heredoc token.
fn parse_line(message: &str, source: &str, original: usize) -> usize {
    if !message.starts_with("invalid ") || !message.contains("indentation") {
        return original;
    }
    let lines: Vec<_> = source.lines().collect();
    for (header, line) in lines.iter().enumerate() {
        let Some((_, label)) = line.split_once("<<<") else {
            continue;
        };
        let label = label.trim().trim_matches(['\'', '"']);
        if label.is_empty() {
            continue;
        }
        let closing = lines.iter().enumerate().skip(header + 1).find(|(_, line)| {
            line.trim_start()
                .strip_prefix(label)
                .is_some_and(|tail| tail.is_empty() || tail.starts_with([';', ',', ')', ']']))
        });
        let Some((closing, closing_line)) = closing else {
            continue;
        };
        let indentation = &closing_line[..closing_line.len() - closing_line.trim_start().len()];
        for (index, body) in lines.iter().enumerate().take(closing).skip(header + 1) {
            if body.trim().is_empty() {
                continue;
            }
            if !body.starts_with(indentation) {
                return index + 1;
            }
        }
    }
    original
}
