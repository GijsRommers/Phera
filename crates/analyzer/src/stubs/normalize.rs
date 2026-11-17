//! Stub declarations may spell language constructs as functions: `function clone(...) {}`.
//! Rename only those pseudo-functions for parsing, then restore their indexed names.

use phera_lexer::{Lexer, TokenKind};

pub fn normalize(file: &str, source: &str, report: &mut String) -> Result<String, String> {
    let mut result = source.to_string();
    for name in ["clone", "exit", "die"] {
        let pattern = format!("function {name}(");
        if result.contains(&pattern) {
            if let Err(error) = phera_parser::parse(&result) {
                let line = phera_source::LineTable::new(&result).line(error.span.start);
                report.push_str(&format!(
                    "{file}:{line}: {}; rename stub-only {name} declaration\n",
                    error.message
                ));
            }
            result = result.replace(&pattern, &format!("function phera_stub_{name}("));
        }
    }
    let mut lexer = Lexer::new(&result);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token().map_err(|e| format!("{file}: {e:?}"))?;
        if token.kind == TokenKind::Eof {
            break;
        }
        tokens.push(token);
    }
    let mut replacements = Vec::new();
    let mut braces = Vec::new();
    let mut awaiting_class = false;
    for (i, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::Class | TokenKind::Interface | TokenKind::Trait | TokenKind::Enum => {
                awaiting_class = true
            }
            TokenKind::LeftBrace => {
                braces.push(awaiting_class);
                awaiting_class = false;
            }
            TokenKind::RightBrace => {
                braces.pop();
            }
            _ => {}
        }
        if token.kind != TokenKind::Function || braces.contains(&true) {
            continue;
        }
        let mut parens = 0;
        let mut opened = false;
        for next in &tokens[i + 1..] {
            match next.kind {
                TokenKind::LeftParen => {
                    parens += 1;
                    opened = true;
                }
                TokenKind::RightParen => parens -= 1,
                TokenKind::LeftBrace if opened && parens == 0 => break,
                TokenKind::Semicolon if opened && parens == 0 => {
                    replacements.push(next.span);
                    break;
                }
                _ => {}
            }
        }
    }
    for span in replacements.into_iter().rev() {
        let line = phera_source::LineTable::new(&result).line(span.start);
        report.push_str(&format!(
            "{file}:{line}: free-function stub ';' replaced by '{{}}'\n"
        ));
        result.replace_range(span.start..span.end, "{}");
    }
    Ok(result)
}
