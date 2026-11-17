//! `prepare('SELECT :id::int, \'?\'')` binds `:id`, preserving quoted SQL and casts.

use crate::errors::DbError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parameter {
    Position(usize),
    Name(String),
}

#[derive(Clone, Debug)]
pub struct Sql {
    pub source: String,
    pub parameters: Vec<Parameter>,
    spans: Vec<(usize, usize)>,
}

impl Sql {
    pub fn parse(source: String) -> Result<Self, DbError> {
        let bytes = source.as_bytes();
        let mut parameters = Vec::new();
        let mut spans = Vec::new();
        let mut index = 0;
        while index < bytes.len() {
            let start = index;
            match bytes[index] {
                b'\'' | b'"' | b'`' => index = quoted(bytes, index),
                b'-' if bytes.get(index + 1) == Some(&b'-') => index = line(bytes, index),
                b'#' => index = line(bytes, index),
                b'/' if bytes.get(index + 1) == Some(&b'*') => index = comment(bytes, index),
                b'$' => index = dollar(bytes, index).unwrap_or(index + 1),
                b':' if bytes.get(index + 1) == Some(&b':') => index += 2,
                b':' if bytes.get(index + 1).is_some_and(|b| name_start(*b)) => {
                    index += 2;
                    while bytes
                        .get(index)
                        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                    {
                        index += 1;
                    }
                    parameters.push(Parameter::Name(source[start + 1..index].to_owned()));
                    spans.push((start, index));
                }
                b'?' => {
                    parameters.push(Parameter::Position(parameters.len()));
                    spans.push((index, index + 1));
                    index += 1;
                }
                _ => index += 1,
            }
        }
        let named = parameters.iter().any(|p| matches!(p, Parameter::Name(_)));
        let positional = parameters
            .iter()
            .any(|p| matches!(p, Parameter::Position(_)));
        if named && positional {
            return Err(DbError::parameter("mixed named and positional parameters"));
        }
        Ok(Self {
            source,
            parameters,
            spans,
        })
    }

    pub fn render(&self, mut placeholder: impl FnMut(usize) -> String) -> String {
        let mut result = String::new();
        let mut previous = 0;
        for (index, &(start, end)) in self.spans.iter().enumerate() {
            result.push_str(&self.source[previous..start]);
            result.push_str(&placeholder(index));
            previous = end;
        }
        result.push_str(&self.source[previous..]);
        result
    }
}

fn name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

pub(crate) fn quoted(bytes: &[u8], start: usize) -> usize {
    let delimiter = bytes[start];
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = (index + 2).min(bytes.len());
        } else if bytes[index] == delimiter {
            index += 1;
            if bytes.get(index) != Some(&delimiter) {
                return index;
            }
            index += 1;
        } else {
            index += 1;
        }
    }
    index
}

pub(crate) fn line(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|b| *b == b'\n')
        .map_or(bytes.len(), |n| start + n + 1)
}

pub(crate) fn comment(bytes: &[u8], start: usize) -> usize {
    let mut depth = 1;
    let mut index = start + 2;
    while index + 1 < bytes.len() {
        match &bytes[index..index + 2] {
            b"/*" => {
                depth += 1;
                index += 2;
            }
            b"*/" => {
                depth -= 1;
                index += 2;
                if depth == 0 {
                    return index;
                }
            }
            _ => index += 1,
        }
    }
    bytes.len()
}

pub(crate) fn dollar(bytes: &[u8], start: usize) -> Option<usize> {
    let mut end = start + 1;
    while bytes
        .get(end)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        end += 1;
    }
    if bytes.get(end) != Some(&b'$') {
        return None;
    }
    let delimiter = &bytes[start..=end];
    let remaining = &bytes[end + 1..];
    Some(
        remaining
            .windows(delimiter.len())
            .position(|s| s == delimiter)
            .map_or(bytes.len(), |n| end + 1 + n + delimiter.len()),
    )
}

#[cfg(test)]
mod tests {
    use super::Sql;

    #[test]
    fn preserves_literals_comments_casts_and_dollar_quotes() {
        let sql = "SELECT :x::int, ':no', \"?\", $$:no$$, $tag$?$tag$ /* ? /* :a */ */ -- :b\n, :y";
        let parsed = Sql::parse(sql.into()).unwrap();
        assert_eq!(parsed.parameters.len(), 2);
        assert_eq!(
            parsed.render(|i| format!("${}", i + 1)),
            sql.replacen(":x", "$1", 1).replacen(":y", "$2", 1)
        );
    }

    #[test]
    fn rejects_mixed_parameters_and_ignores_escaped_quotes() {
        assert!(Sql::parse("SELECT ?, :x".into()).is_err());
        let sql = Sql::parse("SELECT 'it''s ?', 'it\\\'s :x', ?".into()).unwrap();
        assert_eq!(sql.parameters.len(), 1);
    }
}
