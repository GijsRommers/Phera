//! highlight_string('<?= 1 ?>', true) wraps escaped source in PHP's colored pre/code HTML.

use phera_extension_api::{Context, PhpString};
use phera_lexer::{Lexer, TokenKind};

#[derive(Clone, Copy)]
enum Color {
    Html,
    Default,
    Keyword,
    String,
    Comment,
}

impl Color {
    fn for_token(token: TokenKind) -> Option<Self> {
        use TokenKind::*;
        Some(match token {
            Whitespace => return None,
            InlineHtml => Self::Html,
            Comment | DocComment => Self::Comment,
            OpenTag | OpenTagWithEcho | CloseTag | Identifier | QualifiedName
            | FullyQualifiedName | RelativeName | Variable | Integer | Float | NumString | True
            | False | Null | StringVarname | MagicDir | MagicFile | MagicLine | MagicClass
            | MagicFunction | MagicMethod | MagicNamespace | MagicTrait | MagicProperty => {
                Self::Default
            }
            String | DoubleQuote | StringPart => Self::String,
            _ => Self::Keyword,
        })
    }

    fn setting(self) -> (&'static [u8], &'static str) {
        match self {
            Self::Html => (b"highlight.html", "#000000"),
            Self::Default => (b"highlight.default", "#0000BB"),
            Self::Keyword => (b"highlight.keyword", "#007700"),
            Self::String => (b"highlight.string", "#DD0000"),
            Self::Comment => (b"highlight.comment", "#FF8000"),
        }
    }
}

pub(super) fn render(context: &dyn Context, source: &[u8], mut lexer: Lexer<'_>) -> Vec<u8> {
    let colors = [
        Color::Html,
        Color::Default,
        Color::Keyword,
        Color::String,
        Color::Comment,
    ]
    .map(|color| {
        let (name, default) = color.setting();
        context
            .ini_get(name)
            .unwrap_or_else(|| PhpString::from(default))
    });
    let html = &colors[Color::Html as usize];
    let mut current = html;
    let mut output = b"<pre><code style=\"color: ".to_vec();
    output.extend_from_slice(html.as_bytes());
    output.extend_from_slice(b"\">");
    while let Ok(token) = lexer.next_token() {
        if token.kind == TokenKind::Eof {
            break;
        }
        if let Some(color) = Color::for_token(token.kind) {
            let color = &colors[color as usize];
            if color != current {
                if current != html {
                    output.extend_from_slice(b"</span>");
                }
                current = color;
                if color != html {
                    output.extend_from_slice(b"<span style=\"color: ");
                    output.extend_from_slice(color.as_bytes());
                    output.extend_from_slice(b"\">");
                }
            }
        }
        escape(&source[token.span.start..token.span.end], &mut output);
    }
    if current != html {
        output.extend_from_slice(b"</span>");
    }
    output.extend_from_slice(b"</code></pre>");
    output
}

fn escape(text: &[u8], output: &mut Vec<u8>) {
    for &byte in text {
        match byte {
            b'<' => output.extend_from_slice(b"&lt;"),
            b'>' => output.extend_from_slice(b"&gt;"),
            b'&' => output.extend_from_slice(b"&amp;"),
            b'\t' => output.extend_from_slice(b"    "),
            _ => output.push(byte),
        }
    }
}
