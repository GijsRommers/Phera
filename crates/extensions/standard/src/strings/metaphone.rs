//! `metaphone("Knight")` is `"NFT"`: a key for how an English word
//! sounds, ported from php-src ext/standard/metaphone.c.
//!
//! Two quirks of the C code are kept: the input ends at its first NUL byte,
//! and an X, which is two sounds (KS), may make the result one longer than
//! `$max_phonemes`.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use super::{alphabetic, buffer};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[METAPHONE];

/// `metaphone(string $string, int $max_phonemes = 0): string`; 0 means no
/// limit.
///
/// ```php
/// metaphone("Thompson"); // "0MPSN": 0 stands for "th"
/// metaphone("Knight", 2); // "NF"
/// metaphone("Taxi", 2);   // "TKS": the X is not split
/// ```
const METAPHONE: FunctionEntry = FunctionEntry::sync(
    "metaphone",
    &[
        Param::new("string", ParamType::String),
        Param::new("max_phonemes", ParamType::Int).optional(),
    ],
    metaphone,
);

fn vowel(byte: u8) -> bool {
    b"AEIOU".contains(&byte)
}

fn soft(byte: u8) -> bool {
    b"EIY".contains(&byte)
}

fn metaphone(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let limit = args::int(arguments, 1).unwrap_or_default();
    if limit < 0 {
        return Err(NativeError::value_error(
            "metaphone(): Argument #2 ($max_phonemes) must be greater than or equal to 0",
        ));
    }
    let input = args::bytes(arguments, 0);
    let end = input
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(input.len());
    let input = &input[..end];
    let output = buffer(input.len().saturating_mul(2))?;
    let Some(cursor) = input.iter().position(|&byte| alphabetic(byte)) else {
        return Ok(Value::from(output));
    };
    let mut encoder = Encoder {
        input,
        output,
        cursor,
    };
    encoder.initial();
    while encoder.cursor < input.len() && (limit == 0 || encoder.output.len() < limit as usize) {
        encoder.step();
    }
    Ok(Value::from(encoder.output))
}

struct Encoder<'a> {
    input: &'a [u8],
    output: Vec<u8>,
    cursor: usize,
}

impl Encoder<'_> {
    /// PHP's lookahead treats positions beyond the NUL-terminated input as zero.
    fn letter(&self, index: usize) -> u8 {
        self.input
            .get(index)
            .copied()
            .unwrap_or_default()
            .to_ascii_uppercase()
    }

    /// Initial vowels and silent initial consonants have dedicated PHP rules.
    fn initial(&mut self) {
        let current = self.letter(self.cursor);
        let next = self.letter(self.cursor + 1);
        match current {
            b'A' => {
                self.output.push(if next == b'E' { b'E' } else { b'A' });
                self.cursor += if next == b'E' { 2 } else { 1 };
            }
            b'G' | b'K' | b'P' if next == b'N' => {
                self.output.push(b'N');
                self.cursor += 2;
            }
            b'W' if next == b'R' || next == b'H' || vowel(next) => {
                self.output.push(if next == b'R' { b'R' } else { b'W' });
                self.cursor += 2;
            }
            b'X' => {
                self.output.push(b'S');
                self.cursor += 1;
            }
            b'E' | b'I' | b'O' | b'U' => {
                self.output.push(current);
                self.cursor += 1;
            }
            _ => {}
        }
    }

    /// Duplicate consonants are suppressed except C; skipped letters are not rescanned.
    fn step(&mut self) {
        let current = self.letter(self.cursor);
        let previous = self
            .cursor
            .checked_sub(1)
            .map_or(0, |index| self.letter(index));
        let next = self.letter(self.cursor + 1);
        let after = self.letter(self.cursor + 2);
        let mut skip = 0;
        if current.is_ascii_alphabetic() && (current != previous || current == b'C') {
            let phoneme = self.phoneme(current, previous, next, after, &mut skip);
            if let Some(phoneme) = phoneme {
                self.output.push(phoneme);
            }
        }
        self.cursor += 1 + skip;
    }

    /// Dispatch the traditional phoneme rules without interpreting UTF-8.
    fn phoneme(
        &mut self,
        current: u8,
        previous: u8,
        next: u8,
        after: u8,
        skip: &mut usize,
    ) -> Option<u8> {
        match current {
            b'B' if previous != b'M' => Some(b'B'),
            b'C' => self.encode_c(previous, next, after, skip),
            b'D' => self.encode_d(next, after, skip),
            b'G' => self.encode_g(previous, next, after, skip),
            b'H' if vowel(next) && !b"CGPST".contains(&previous) => Some(b'H'),
            b'K' if previous != b'C' => Some(b'K'),
            b'P' => Some(if next == b'H' { b'F' } else { b'P' }),
            b'Q' => Some(b'K'),
            b'S' => self.encode_s(next, after, skip),
            b'T' => self.encode_t(next, after, skip),
            b'V' => Some(b'F'),
            b'W' | b'Y' if vowel(next) => Some(current),
            b'X' => {
                self.output.push(b'K');
                Some(b'S')
            }
            b'Z' => Some(b'S'),
            b'F' | b'J' | b'L' | b'M' | b'N' | b'R' => Some(current),
            _ => None,
        }
    }

    /// Preserve PHP metaphone's context-sensitive C rule.
    fn encode_c(&self, previous: u8, next: u8, after: u8, skip: &mut usize) -> Option<u8> {
        if soft(next) {
            if next == b'I' && after == b'A' {
                Some(b'X')
            } else if previous == b'S' {
                None
            } else {
                Some(b'S')
            }
        } else if next == b'H' {
            *skip = 1;
            Some(b'X')
        } else {
            Some(b'K')
        }
    }

    /// Preserve PHP metaphone's context-sensitive D rule.
    fn encode_d(&self, next: u8, after: u8, skip: &mut usize) -> Option<u8> {
        if next == b'G' && soft(after) {
            *skip = 1;
            Some(b'J')
        } else {
            Some(b'T')
        }
    }

    /// Preserve PHP metaphone's context-sensitive G rule.
    fn encode_g(&self, previous: u8, next: u8, after: u8, skip: &mut usize) -> Option<u8> {
        if next == b'H' {
            let back = self
                .cursor
                .checked_sub(3)
                .map_or(0, |index| self.letter(index));
            let back4 = self
                .cursor
                .checked_sub(4)
                .map_or(0, |index| self.letter(index));
            if !b"BDH".contains(&back) && back4 != b'H' {
                *skip = 1;
                Some(b'F')
            } else {
                None
            }
        } else if next == b'N' {
            if !alphabetic(after) || after == b'E' && self.letter(self.cursor + 3) == b'D' {
                None
            } else {
                Some(b'K')
            }
        } else if soft(next) && previous != b'G' {
            Some(b'J')
        } else {
            Some(b'K')
        }
    }

    /// Preserve PHP metaphone's context-sensitive S rule.
    fn encode_s(&self, next: u8, after: u8, skip: &mut usize) -> Option<u8> {
        if next == b'I' && matches!(after, b'O' | b'A') {
            Some(b'X')
        } else if next == b'H' {
            *skip = 1;
            Some(b'X')
        } else {
            Some(b'S')
        }
    }

    /// Preserve PHP metaphone's context-sensitive T rule.
    fn encode_t(&self, next: u8, after: u8, skip: &mut usize) -> Option<u8> {
        if next == b'I' && matches!(after, b'O' | b'A') {
            Some(b'X')
        } else if next == b'H' {
            *skip = 1;
            Some(b'0')
        } else if next == b'C' && after == b'H' {
            None
        } else {
            Some(b'T')
        }
    }
}
