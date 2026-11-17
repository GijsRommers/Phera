//! Expand earlier assignments once. `HOST=${HOST:-localhost}` never looks ahead.

use super::{FILE_LIMIT, ParseError, Parser, VALUE_LIMIT, name_byte, valid_name};

impl Parser<'_> {
    pub(super) fn expand(&mut self, out: &mut Vec<u8>) -> Result<(), ParseError> {
        let start = self.pos;
        self.pos += 1;
        if self.peek() == Some(b'(') {
            return Err(self.error("a variable expansion (command substitution is disabled)"));
        }
        let braced = self.peek() == Some(b'{');
        self.pos += usize::from(braced);
        // phpdotenv permits indirect names such as ${${KEY}}. Consume and
        // unwind the braces iteratively so nesting never grows the Rust stack.
        let mut indirect = 0;
        while braced && self.rest().starts_with(b"${") {
            if indirect >= 64 {
                return Err(self.error("at most 64 indirect names"));
            }
            indirect += 1;
            self.pos += 2;
        }
        let name_start = self.pos;
        while self.peek().is_some_and(|b| {
            if braced {
                name_byte(b)
            } else {
                b.is_ascii_alphanumeric() || b == b'_'
            }
        }) {
            self.pos += 1;
        }
        let name = &self.bytes[name_start..self.pos];
        if !braced && (name.is_empty() || name[0].is_ascii_digit()) {
            self.pos = start + 1;
            out.push(b'$');
            return Ok(());
        }
        if braced && !valid_name(name) {
            return Err(self.error("a variable name inside braces"));
        }
        let default =
            if braced && (self.rest().starts_with(b":-") || self.rest().starts_with(b":=")) {
                let assign = self.bytes[self.pos + 1] == b'=';
                self.pos += 2;
                let begin = self.pos;
                while let Some(b) = self.peek() {
                    if b == b'}' {
                        break;
                    }
                    if matches!(b, b'\r' | b'\n' | b'$' | b'{' | b'\'' | b'"' | 0) {
                        return Err(self.error("a literal default followed by '}'"));
                    }
                    self.pos += 1;
                }
                Some((assign, &self.bytes[begin..self.pos]))
            } else {
                None
            };
        if braced {
            if self.peek() != Some(b'}') {
                return Err(self.error("'}' after the expansion"));
            }
            self.pos += 1;
        }

        let value = self.external.get(name).or_else(|| self.files.get(name));
        let fallback = default.filter(|_| value.is_none_or(<[u8]>::is_empty));
        let mut replacement = match (fallback, value) {
            (Some((_, literal)), _) => literal,
            (_, Some(value)) => value,
            _ if braced => &self.bytes[start..self.pos],
            _ => &[],
        };
        for _ in 0..indirect {
            if self.peek() != Some(b'}') {
                return Err(self.error("a closing expansion brace"));
            }
            self.pos += 1;
            replacement = self
                .external
                .get(replacement)
                .or_else(|| self.files.get(replacement))
                .unwrap_or(&self.bytes[start..self.pos]);
        }
        self.expanded += replacement.len();
        if out.len() + replacement.len() > VALUE_LIMIT || self.expanded > FILE_LIMIT {
            return Err(self.error("an expansion within the size limit"));
        }
        out.extend_from_slice(replacement);
        if let Some((true, literal)) = fallback
            && self.external.get(name).is_none()
        {
            self.files.insert(name.to_vec(), literal.to_vec());
        }
        Ok(())
    }
}
