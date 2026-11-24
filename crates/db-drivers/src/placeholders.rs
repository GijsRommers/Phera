//! PDO rewrites placeholders in SQL code and leaves each driver's quoted regions alone.

use std::borrow::Cow;

use crate::errors::DbError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parameter {
    Position(usize),
    Name(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syntax {
    Postgres {
        backslash_strings: bool,
    },
    Mysql {
        backslash_strings: bool,
        ansi_quotes: bool,
    },
}

impl Syntax {
    pub const POSTGRES: Self = Self::Postgres {
        backslash_strings: false,
    };
    pub const MYSQL: Self = Self::Mysql {
        backslash_strings: true,
        ansi_quotes: false,
    };

    pub fn backslash_strings(self) -> bool {
        match self {
            Self::Postgres { backslash_strings }
            | Self::Mysql {
                backslash_strings, ..
            } => backslash_strings,
        }
    }

    fn region(self, bytes: &[u8], at: usize) -> Option<Region> {
        match self {
            Self::Postgres { backslash_strings } => postgres(bytes, at, backslash_strings),
            Self::Mysql {
                backslash_strings,
                ansi_quotes,
            } => mysql(bytes, at, backslash_strings, ansi_quotes),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Sql {
    pub source: String,
    pub parameters: Vec<Parameter>,
    edits: Vec<Edit>,
    string_mode: Option<Syntax>,
    escaped_questions: bool,
    unterminated_with_marker: bool,
    /// Native PostgreSQL tokens are not PDO placeholders and must not be rewritten.
    pub has_dollar_parameters: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Edit {
    start: usize,
    end: usize,
    parameter: Option<usize>,
}

impl Sql {
    /// SQLite binds by the native prepared statement's parameter metadata.
    pub fn sqlite(source: String) -> Self {
        Self {
            source,
            parameters: Vec::new(),
            edits: Vec::new(),
            string_mode: None,
            escaped_questions: false,
            unterminated_with_marker: false,
            has_dollar_parameters: false,
        }
    }

    pub fn parse(source: String, syntax: Syntax) -> Result<Self, DbError> {
        let mut sql = Self::sqlite(source);
        sql.string_mode = Self::needs_string_mode(&sql.source).then_some(syntax);
        let bytes = sql.source.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            if let Some(region) = syntax.region(bytes, at) {
                if region.unterminated_quote {
                    sql.unterminated_with_marker = bytes[at..].contains(&b'?')
                        || bytes[at..]
                            .windows(2)
                            .any(|pair| pair[0] == b':' && name_start(pair[1]));
                }
                at = region.end;
                continue;
            }
            let start = at;
            let parameter = match bytes[at] {
                b':' if bytes.get(at + 1) == Some(&b':') => {
                    at += 2;
                    while bytes.get(at) == Some(&b':') {
                        at += 1;
                    }
                    continue;
                }
                b':' if bytes.get(at + 1).is_some_and(|&byte| name_start(byte)) => {
                    at += 2;
                    while bytes.get(at).is_some_and(|&byte| name_part(byte)) {
                        at += 1;
                    }
                    Parameter::Name(sql.source[start + 1..at].to_owned())
                }
                b'?' if matches!(syntax, Syntax::Postgres { .. })
                    && bytes.get(at + 1) == Some(&b'?') =>
                {
                    at += 2;
                    sql.escaped_questions = true;
                    sql.edits.push(Edit {
                        start,
                        end: at,
                        parameter: None,
                    });
                    continue;
                }
                b'?' => {
                    at += 1;
                    Parameter::Position(sql.parameters.len())
                }
                b'$' if matches!(syntax, Syntax::Postgres { .. })
                    && postgres_token_start(bytes, at) =>
                {
                    sql.has_dollar_parameters |= bytes.get(at + 1).is_some_and(u8::is_ascii_digit);
                    at += 1;
                    continue;
                }
                _ => {
                    at += 1;
                    continue;
                }
            };
            sql.edits.push(Edit {
                start,
                end: at,
                parameter: Some(sql.parameters.len()),
            });
            sql.parameters.push(parameter);
        }
        if sql
            .parameters
            .iter()
            .any(|p| matches!(p, Parameter::Name(_)))
            && sql
                .parameters
                .iter()
                .any(|p| matches!(p, Parameter::Position(_)))
        {
            return Err(DbError::parameter("mixed named and positional parameters"));
        }
        Ok(sql)
    }

    /// Backslashes immediately before a quote can change placeholder visibility
    /// when a connection uses different string rules.
    pub fn needs_string_mode(source: &str) -> bool {
        let Some(first) = source.find('\\') else {
            return false;
        };
        let mut odd_backslashes = false;
        for byte in source.as_bytes()[first..].iter().copied() {
            if byte == b'\\' {
                odd_backslashes = !odd_backslashes;
            } else {
                if odd_backslashes && matches!(byte, b'\'' | b'"') {
                    return true;
                }
                odd_backslashes = false;
            }
        }
        false
    }

    /// Emulation cannot interpolate into an unclosed literal.
    pub fn check_emulation(&self) -> Result<(), DbError> {
        if self.unterminated_with_marker {
            return Err(DbError::parameter(
                "number of bound variables does not match number of tokens",
            ));
        }
        Ok(())
    }

    pub fn has_sensitive_quotes(&self) -> bool {
        self.string_mode.is_some()
    }

    pub fn has_only_unterminated_markers(&self) -> bool {
        self.unterminated_with_marker && self.parameters.is_empty()
    }

    pub fn check_string_mode(&self, syntax: Syntax) -> Result<(), DbError> {
        if self.string_mode.is_none_or(|prior| prior == syntax) {
            return Ok(());
        }
        let current = Self::parse(self.source.clone(), syntax)?;
        if self.edits != current.edits
            || self.unterminated_with_marker != current.unterminated_with_marker
        {
            return Err(DbError::parameter(
                "number of bound variables does not match number of tokens",
            ));
        }
        current.check_emulation()
    }

    pub fn render(&self, mut placeholder: impl FnMut(usize) -> String) -> String {
        self.rewrite(|index, _, output| output.push_str(&placeholder(index)))
    }

    /// Rewrites placeholders directly into the destination buffer without
    /// allocating an intermediate `String` per parameter.
    pub fn render_into(&self, mut placeholder: impl FnMut(usize, &mut String)) -> String {
        self.rewrite(|index, _, output| placeholder(index, output))
    }

    pub fn unbound_sql(&self) -> Cow<'_, str> {
        if self.escaped_questions {
            Cow::Owned(self.rewrite(|_, token, output| output.push_str(token)))
        } else {
            Cow::Borrowed(&self.source)
        }
    }

    fn rewrite(&self, mut replacement: impl FnMut(usize, &str, &mut String)) -> String {
        let mut result = String::with_capacity(self.source.len());
        let mut previous = 0;
        for edit in &self.edits {
            result.push_str(&self.source[previous..edit.start]);
            if let Some(index) = edit.parameter {
                replacement(index, &self.source[edit.start..edit.end], &mut result);
            } else {
                result.push('?');
            }
            previous = edit.end;
        }
        result.push_str(&self.source[previous..]);
        result
    }
}

fn name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn name_part(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn postgres_token_start(bytes: &[u8], at: usize) -> bool {
    at == 0 || !postgres_identifier_part(bytes[at - 1])
}

fn postgres_identifier_part(byte: u8) -> bool {
    name_part(byte) || byte == b'$' || !byte.is_ascii()
}

struct Region {
    end: usize,
    unterminated_quote: bool,
}

impl Region {
    fn opaque(end: usize) -> Self {
        Self {
            end,
            unterminated_quote: false,
        }
    }
}

fn postgres(bytes: &[u8], at: usize, backslashes: bool) -> Option<Region> {
    let next = bytes.get(at + 1).copied();
    match bytes[at] {
        b'\'' => Some(quoted(bytes, at, backslashes)),
        b'"' => Some(quoted(bytes, at, false)),
        b'E' | b'e' if next == Some(b'\'') && postgres_token_start(bytes, at) => {
            Some(quoted(bytes, at + 1, true))
        }
        b'-' if next == Some(b'-') => Some(Region::opaque(line(bytes, at))),
        b'/' if next == Some(b'*') => Some(Region::opaque(comment(bytes, at, true))),
        b'$' if postgres_token_start(bytes, at) => dollar(bytes, at),
        _ => None,
    }
}

fn mysql(bytes: &[u8], at: usize, backslashes: bool, ansi_quotes: bool) -> Option<Region> {
    let next = bytes.get(at + 1).copied();
    match bytes[at] {
        b'\'' => Some(quoted(bytes, at, backslashes)),
        b'"' => Some(quoted(bytes, at, backslashes && !ansi_quotes)),
        b'\x60' => Some(quoted(bytes, at, false)),
        b'#' => Some(Region::opaque(line(bytes, at))),
        b'-' if next == Some(b'-')
            && bytes
                .get(at + 2)
                .is_some_and(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control()) =>
        {
            Some(Region::opaque(line(bytes, at)))
        }
        b'/' if next == Some(b'*') => Some(Region::opaque(comment(bytes, at, false))),
        _ => None,
    }
}

fn quoted(bytes: &[u8], start: usize, backslashes: bool) -> Region {
    let delimiter = bytes[start];
    let mut at = start + 1;
    while at < bytes.len() {
        if bytes[at] == b'\\' && backslashes {
            at = (at + 2).min(bytes.len());
        } else if bytes[at] == delimiter {
            at += 1;
            if bytes.get(at) != Some(&delimiter) {
                return Region::opaque(at);
            }
            at += 1;
        } else {
            at += 1;
        }
    }
    Region {
        end: at,
        unterminated_quote: true,
    }
}

fn line(bytes: &[u8], at: usize) -> usize {
    bytes[at..]
        .iter()
        .position(|byte| matches!(byte, b'\n' | b'\r'))
        .map_or(bytes.len(), |length| at + length + 1)
}

fn comment(bytes: &[u8], start: usize, nested: bool) -> usize {
    let mut depth = 1;
    let mut at = start + 2;
    while at + 1 < bytes.len() {
        match &bytes[at..at + 2] {
            b"/*" if nested => {
                depth += 1;
                at += 2;
            }
            b"*/" => {
                depth -= 1;
                at += 2;
                if depth == 0 {
                    return at;
                }
            }
            _ => at += 1,
        }
    }
    bytes.len()
}

fn dollar(bytes: &[u8], start: usize) -> Option<Region> {
    let mut end = start + 1;
    if bytes.get(end) != Some(&b'$') {
        if !bytes
            .get(end)
            .is_some_and(|&byte| name_start(byte) || !byte.is_ascii())
        {
            return None;
        }
        while bytes
            .get(end)
            .is_some_and(|&byte| name_part(byte) || !byte.is_ascii())
        {
            end += 1;
        }
    }
    if bytes.get(end) != Some(&b'$') {
        return None;
    }
    let delimiter = &bytes[start..=end];
    let remaining = &bytes[end + 1..];
    Some(
        remaining
            .windows(delimiter.len())
            .position(|window| window == delimiter)
            .map_or(
                Region {
                    end: bytes.len(),
                    unterminated_quote: true,
                },
                |length| Region::opaque(end + 1 + length + delimiter.len()),
            ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_preserves_regions_and_rewrites_question_escapes() {
        let sql = Sql::parse(
            r#"SELECT :x::int, E'it\'s :v', "a""?", $tag$ :v $tag$, col$1, a ?? 'b' /* /* :z */ :tail */ -- :end
, :y"#
                .into(),
            Syntax::POSTGRES,
        )
        .unwrap();
        assert_eq!(
            sql.parameters,
            [Parameter::Name("x".into()), Parameter::Name("y".into())]
        );
        assert!(!sql.has_dollar_parameters);
        assert!(
            sql.render(|index| format!("{}{}", '$', index + 1))
                .ends_with(", $2")
        );
        assert!(sql.unbound_sql().contains("a ? 'b'"));
    }

    #[test]
    fn mysql_uses_its_own_identifier_and_comment_rules() {
        let sql = Sql::parse(
            "SELECT 4--? AS a, 1 AS \x60?:v\x60, /* /* ? */ ? # :ignored\n".into(),
            Syntax::MYSQL,
        )
        .unwrap();
        assert_eq!(
            sql.parameters,
            [Parameter::Position(0), Parameter::Position(1)]
        );
    }

    #[test]
    fn emulation_requires_the_same_placeholders_after_a_mode_change() {
        assert!(!Sql::needs_string_mode("SELECT 'a\\\\' AS x, ? AS y"));
        let sql = Sql::parse("SELECT 'a\\' AS x, ? AS y".into(), Syntax::POSTGRES).unwrap();
        assert!(sql.check_string_mode(Syntax::POSTGRES).is_ok());
        assert!(
            sql.check_string_mode(Syntax::Postgres {
                backslash_strings: true,
            })
            .is_err()
        );
    }
}
