//! Tokenize `array{id?: int, name: 'a'}` without treating its punctuation as PHP code.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token<'a> {
    Word(&'a str),
    Variable(&'a str),
    Quoted(&'a str),
    Punctuation(char),
    Dots,
    DoubleColon,
    UnclosedQuote,
}

pub struct Lexeme<'a> {
    pub token: Token<'a>,
    pub offset: usize,
}

pub fn lex(source: &str) -> Vec<Lexeme<'_>> {
    let mut result = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some((offset, ch)) = chars.next() {
        if ch.is_whitespace() {
            continue;
        }
        let rest = &source[offset..];
        if rest.starts_with("//") {
            while chars.peek().is_some_and(|(_, ch)| *ch != '\n') {
                chars.next();
            }
            continue;
        }
        let token = match ch {
            '.' if rest.starts_with("...") => {
                chars.next();
                chars.next();
                Token::Dots
            }
            ':' if rest.starts_with("::") => {
                chars.next();
                Token::DoubleColon
            }
            '\'' | '"' => {
                let mut end = None;
                while let Some((at, next)) = chars.next() {
                    if next == '\\' {
                        chars.next();
                    } else if next == ch {
                        end = Some(at);
                        break;
                    }
                }
                end.map_or(Token::UnclosedQuote, |end| {
                    Token::Quoted(&source[offset + 1..end])
                })
            }
            '$' | '\\' | '_' | '-' => word(source, offset, ch, &mut chars),
            ch if ch.is_alphanumeric() || !ch.is_ascii() => word(source, offset, ch, &mut chars),
            ch => Token::Punctuation(ch),
        };
        result.push(Lexeme { token, offset });
    }
    result
}

fn word<'a>(
    source: &'a str,
    offset: usize,
    first: char,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'a>>,
) -> Token<'a> {
    while chars.peek().is_some_and(|(_, ch)| {
        ch.is_alphanumeric()
            || matches!(ch, '_' | '\\' | '-')
            || !ch.is_ascii() && !ch.is_whitespace()
    }) {
        chars.next();
    }
    let end = chars.peek().map_or(source.len(), |(offset, _)| *offset);
    if first == '$' {
        Token::Variable(&source[offset + 1..end])
    } else {
        Token::Word(&source[offset..end])
    }
}
