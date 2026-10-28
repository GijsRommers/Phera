//! Matching for `--EXPECTF--` sections, following php-src's `run-tests.php`.

use std::collections::HashSet;

#[derive(Debug)]
enum Piece {
    Literal(char),
    Placeholder(Placeholder),
}

#[derive(Debug, Clone, Copy)]
enum Placeholder {
    /// `%s`: one or more characters, excluding newlines.
    Line,
    /// `%S`: zero or more characters, excluding newlines.
    LineOrEmpty,
    /// `%a`: one or more characters, including newlines.
    Any,
    /// `%A`: zero or more characters, including newlines.
    AnyOrEmpty,
    /// `%w`: zero or more whitespace characters.
    Whitespace,
    /// `%i`: a signed integer.
    SignedInt,
    /// `%d`: an unsigned integer.
    UnsignedInt,
    /// `%x`: hexadecimal digits.
    Hex,
    /// `%f`: a float.
    Float,
    /// `%c`: exactly one character.
    Char,
}

impl Placeholder {
    fn min_len(self) -> usize {
        match self {
            Self::LineOrEmpty | Self::AnyOrEmpty | Self::Whitespace => 0,
            _ => 1,
        }
    }

    fn accepts_char(self, c: char) -> bool {
        match self {
            Self::Line | Self::LineOrEmpty => c != '\n' && c != '\r',
            Self::Any | Self::AnyOrEmpty | Self::Char => true,
            Self::Whitespace => c.is_whitespace(),
            Self::SignedInt => c.is_ascii_digit() || c == '+' || c == '-',
            Self::UnsignedInt => c.is_ascii_digit(),
            Self::Hex => c.is_ascii_hexdigit(),
            Self::Float => c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'),
        }
    }

    fn accepts(self, text: &[char]) -> bool {
        match self {
            Self::Char => text.len() == 1,
            Self::SignedInt => is_signed_int(text),
            Self::Float => is_float(text),
            _ => true,
        }
    }
}

/// `[+-]?\d+`
fn is_signed_int(text: &[char]) -> bool {
    let digits = match text.first() {
        Some('+' | '-') => &text[1..],
        _ => text,
    };
    !digits.is_empty() && digits.iter().all(char::is_ascii_digit)
}

/// `[+-]?\.?\d+\.?\d*(?:[Ee][+-]?\d+)?`
fn is_float(text: &[char]) -> bool {
    let mut i = 0;
    let at = |i: usize| text.get(i).copied();
    let digits = |mut i: usize| {
        while at(i).is_some_and(|c| c.is_ascii_digit()) {
            i += 1;
        }
        i
    };

    if matches!(at(i), Some('+' | '-')) {
        i += 1;
    }
    if at(i) == Some('.') {
        i += 1;
    }

    let end = digits(i);
    if end == i {
        return false;
    }
    i = end;

    if at(i) == Some('.') {
        i = digits(i + 1);
    }

    if matches!(at(i), Some('e' | 'E')) {
        let mut j = i + 1;
        if matches!(at(j), Some('+' | '-')) {
            j += 1;
        }
        let end = digits(j);
        if end == j {
            return false;
        }
        i = end;
    }

    i == text.len()
}

fn compile(pattern: &str) -> Result<Vec<Piece>, String> {
    let mut pieces = Vec::new();
    let mut chars = pattern.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '%' {
            pieces.push(Piece::Literal(c));
            continue;
        }

        let placeholder = match chars.peek() {
            Some('s') => Placeholder::Line,
            Some('S') => Placeholder::LineOrEmpty,
            Some('a') => Placeholder::Any,
            Some('A') => Placeholder::AnyOrEmpty,
            Some('w') => Placeholder::Whitespace,
            Some('i') => Placeholder::SignedInt,
            Some('d') => Placeholder::UnsignedInt,
            Some('x') => Placeholder::Hex,
            Some('f') => Placeholder::Float,
            Some('c') => Placeholder::Char,
            Some('e') => {
                chars.next();
                pieces.push(Piece::Literal('/'));
                continue;
            }
            Some('0') => {
                chars.next();
                pieces.push(Piece::Literal('\0'));
                continue;
            }
            Some('r') => return Err("%r...%r regex placeholders are not supported".into()),
            _ => {
                pieces.push(Piece::Literal('%'));
                continue;
            }
        };

        chars.next();
        pieces.push(Piece::Placeholder(placeholder));
    }

    Ok(pieces)
}

/// Whether `output` matches an `--EXPECTF--` `pattern`.
///
/// Both are trimmed and have `\r\n` normalized to `\n` first, as `run-tests.php`
/// does. Returns an error for patterns using unsupported placeholders.
pub fn expectf_matches(pattern: &str, output: &str) -> Result<bool, String> {
    let pattern = pattern.replace("\r\n", "\n");
    let output: Vec<char> = output.replace("\r\n", "\n").trim().chars().collect();
    let pieces = compile(pattern.trim())?;

    let mut matcher = Matcher {
        pieces: &pieces,
        output: &output,
        failed: HashSet::new(),
    };

    Ok(matcher.matches(0, 0))
}

struct Matcher<'a> {
    pieces: &'a [Piece],
    output: &'a [char],
    /// `(piece, position)` states already known not to match.
    failed: HashSet<(usize, usize)>,
}

impl Matcher<'_> {
    fn matches(&mut self, piece: usize, pos: usize) -> bool {
        let Some(current) = self.pieces.get(piece) else {
            return pos == self.output.len();
        };

        if self.failed.contains(&(piece, pos)) {
            return false;
        }

        let matched = match current {
            Piece::Literal(c) => {
                self.output.get(pos) == Some(c) && self.matches(piece + 1, pos + 1)
            }
            Piece::Placeholder(p) => {
                let run = self.output[pos..]
                    .iter()
                    .take_while(|&&c| p.accepts_char(c))
                    .count();
                let max = if matches!(p, Placeholder::Char) {
                    run.min(1)
                } else {
                    run
                };

                (p.min_len()..=max).rev().any(|len| {
                    p.accepts(&self.output[pos..pos + len]) && self.matches(piece + 1, pos + len)
                })
            }
        };

        if !matched {
            self.failed.insert((piece, pos));
        }

        matched
    }
}

#[cfg(test)]
mod tests {
    use super::expectf_matches;

    fn ok(pattern: &str, output: &str) -> bool {
        expectf_matches(pattern, output).unwrap()
    }

    #[test]
    fn literal_text() {
        assert!(ok("hello", "hello"));
        assert!(ok("hello\n", "  hello  "));
        assert!(!ok("hello", "hello!"));
    }

    #[test]
    fn line_placeholders() {
        assert!(ok(
            "Warning: %s in %s on line %d",
            "Warning: oops in /a/b.php on line 3"
        ));
        assert!(!ok("a%sb", "ab"));
        assert!(ok("a%Sb", "ab"));
        assert!(!ok("a%sb", "a\nb"));
    }

    #[test]
    fn any_placeholders() {
        assert!(ok("start%aend", "start\nmiddle\nend"));
        assert!(ok("start%Aend", "startend"));
        assert!(!ok("start%aend", "startend"));
    }

    #[test]
    fn number_placeholders() {
        assert!(ok("int(%i)", "int(-42)"));
        assert!(ok("int(%d)", "int(42)"));
        assert!(!ok("int(%d)", "int(-42)"));
        assert!(ok("float(%f)", "float(3.14)"));
        assert!(ok("float(%f)", "float(-1.5E+20)"));
        assert!(ok("0x%x", "0xdeadBEEF"));
    }

    #[test]
    fn other_placeholders() {
        assert!(ok("[%c]", "[x]"));
        assert!(!ok("[%c]", "[xy]"));
        assert!(ok("a%wb", "a \t b"));
        assert!(ok("%e", "/"));
        assert!(ok("100%", "100%"));
    }

    #[test]
    fn regex_is_unsupported() {
        assert!(expectf_matches("%r.*%r", "x").is_err());
    }
}
