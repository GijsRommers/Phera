//! `strtok`: cut a string into tokens, one per call.
//!
//! ```php
//! $word = strtok("one two  three", " "); // "one"
//! $word = strtok(" ");                   // "two"
//! $word = strtok(" ");                   // "three"
//! $word = strtok(" ");                   // false
//! ```
//!
//! The first call gives the string; later calls give only the delimiters
//! and continue in the same string, which the request remembers.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[STRTOK];

/// The string being cut and how far the cutting has come.
#[derive(Default)]
struct Tokenizer {
    string: Vec<u8>,
    position: usize,
}

impl Tokenizer {
    /// The next token: the bytes up to the next delimiter, after skipping
    /// the delimiters in front of it. So empty tokens do not exist:
    /// `"a,,b"` cut at `","` is `"a"` and `"b"`.
    fn next_token(&mut self, delimiters: &[u8]) -> Option<&[u8]> {
        let is_delimiter = |byte: &u8| delimiters.contains(byte);
        let rest = &self.string[self.position..];
        let start = self.position + rest.iter().position(|byte| !is_delimiter(byte))?;
        let length = self.string[start..]
            .iter()
            .position(is_delimiter)
            .unwrap_or(self.string.len() - start);
        let end = start + length;
        // The delimiter that ended the token is used up with it.
        self.position = (end + 1).min(self.string.len());
        Some(&self.string[start..end])
    }
}

/// `strtok(string $string, ?string $token = null): string|false`
///
/// With one argument, that argument is the list of delimiters.
const STRTOK: FunctionEntry = FunctionEntry::sync(
    "strtok",
    &[
        Param::new("string", ParamType::String),
        Param::new("token", ParamType::String).nullable().optional(),
    ],
    strtok,
);

fn strtok(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let first = args::bytes(arguments, 0).to_vec();
    let second = args::string(arguments, 1).map(|token| token.as_bytes().to_vec());
    let tokenizer = context.state().get_or_default::<Tokenizer>();
    let delimiters = match second {
        // Two arguments start on a new string.
        Some(delimiters) => {
            *tokenizer = Tokenizer {
                string: first,
                position: 0,
            };
            delimiters
        }
        None => first,
    };
    Ok(match tokenizer.next_token(&delimiters) {
        Some(token) => Value::from(token),
        None => {
            // Nothing is left; PHP forgets the string here.
            *tokenizer = Tokenizer::default();
            Value::Bool(false)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(string: &str, delimiters: &str) -> Vec<String> {
        let mut tokenizer = Tokenizer {
            string: string.as_bytes().to_vec(),
            position: 0,
        };
        let mut found = Vec::new();
        while let Some(token) = tokenizer.next_token(delimiters.as_bytes()) {
            found.push(String::from_utf8_lossy(token).into_owned());
        }
        found
    }

    #[test]
    fn delimiters_in_a_row_make_no_empty_tokens() {
        assert_eq!(tokens("  a, b;;c ", " ,;"), ["a", "b", "c"]);
        assert!(tokens(",,,", ",").is_empty());
        assert!(tokens("", ",").is_empty());
    }

    #[test]
    fn without_delimiters_the_rest_is_one_token() {
        assert_eq!(tokens("abc", ""), ["abc"]);
    }
}
