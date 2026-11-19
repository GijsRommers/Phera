//! php_strip_whitespace() removes PHP comments but retains HTML and string contents.

use phera_lexer::{Lexer, TokenKind};

pub(super) fn source(source: &[u8], mut lexer: Lexer<'_>) -> Vec<u8> {
    let mut output = Vec::new();
    let mut spaced = false;
    while let Ok(token) = lexer.next_token() {
        match token.kind {
            TokenKind::Eof => break,
            TokenKind::Whitespace => {
                if !spaced {
                    output.push(b' ');
                    spaced = true;
                }
            }
            TokenKind::Comment | TokenKind::DocComment => {}
            TokenKind::EndHeredoc => {
                output.extend_from_slice(&source[token.span.start..token.span.end]);
                if let Ok(next) = lexer.next_token()
                    && !matches!(next.kind, TokenKind::Whitespace | TokenKind::Eof)
                {
                    output.extend_from_slice(&source[next.span.start..next.span.end]);
                }
                output.push(b'\n');
                spaced = true;
            }
            _ => {
                output.extend_from_slice(&source[token.span.start..token.span.end]);
                spaced = false;
            }
        }
    }
    output
}
