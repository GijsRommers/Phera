//! Byte scanning preserves PHP's prefix matches: `2024` is a time before it is a year.
use super::token::{Action, Candidate};

#[derive(Clone, Copy)]
pub struct Cursor<'a> {
    pub input: &'a [u8],
    pub pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self { input, pos: 0 }
    }
    pub fn peek(self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }
    pub fn literal(&mut self, text: &[u8]) -> Option<()> {
        if self.input.get(self.pos..self.pos + text.len())? != text {
            return None;
        }
        self.pos += text.len();
        Some(())
    }
    pub fn word(&mut self, text: &[u8]) -> Option<()> {
        if !self
            .input
            .get(self.pos..self.pos + text.len())?
            .eq_ignore_ascii_case(text)
        {
            return None;
        }
        self.pos += text.len();
        Some(())
    }
    pub fn one(&mut self, bytes: &[u8]) -> Option<u8> {
        let byte = self.peek()?;
        if !bytes.contains(&byte) {
            return None;
        }
        self.pos += 1;
        Some(byte)
    }
    pub fn skip(&mut self, bytes: &[u8]) -> usize {
        let start = self.pos;
        while self.peek().is_some_and(|b| bytes.contains(&b)) {
            self.pos += 1;
        }
        self.pos - start
    }
    pub fn space(&mut self) -> Option<()> {
        if self.skip(b" \t") > 0 {
            return Some(());
        }
        for bytes in [b"\xc2\xa0".as_slice(), b"\xe2\x80\xaf"] {
            if self.literal(bytes).is_some() {
                while self.literal(bytes).is_some() {}
                return Some(());
            }
        }
        None
    }
    pub fn number(&mut self, min: usize, max: usize, limit: i64) -> Option<i64> {
        let start = self.pos;
        let mut value = 0i64;
        while self.pos - start < max {
            let Some(byte) = self.peek().filter(u8::is_ascii_digit) else {
                break;
            };
            let Some(next) = value
                .checked_mul(10)
                .and_then(|v| v.checked_add((byte - b'0').into()))
                .filter(|v| *v <= limit)
            else {
                break;
            };
            value = next;
            self.pos += 1;
        }
        (self.pos - start >= min).then_some(value)
    }
    pub fn day(&mut self, padded: bool) -> Option<i64> {
        let day = self.number(if padded { 2 } else { 1 }, 2, 31)?;
        if !padded {
            for suffix in [b"st", b"nd", b"rd", b"th"] {
                if self.literal(suffix).is_some() {
                    break;
                }
            }
        }
        Some(day)
    }
    pub fn year(&mut self, min: usize, max: usize, adjust: bool) -> Option<i64> {
        let start = self.pos;
        let year = self.number(min, max, 9999)?;
        Some(if adjust && self.pos - start < 4 && year < 100 {
            year + if year < 70 { 2000 } else { 1900 }
        } else {
            year
        })
    }
    pub fn month(&mut self, abbreviations: bool, roman: bool) -> Option<i64> {
        if !self.peek()?.is_ascii_alphabetic() {
            return None;
        }
        let mut best = None;
        for (index, name) in [
            "january",
            "february",
            "march",
            "april",
            "may",
            "june",
            "july",
            "august",
            "september",
            "october",
            "november",
            "december",
        ]
        .iter()
        .enumerate()
        {
            for len in [3, if abbreviations { 3 } else { name.len() }] {
                let mut cursor = *self;
                if cursor.word(&name.as_bytes()[..len]).is_some()
                    && best.is_none_or(|(end, _)| cursor.pos > end)
                {
                    best = Some((cursor.pos, index as i64 + 1));
                }
            }
        }
        let mut cursor = *self;
        if cursor.word(b"sept").is_some() && best.is_none_or(|(end, _)| cursor.pos > end) {
            best = Some((cursor.pos, 9));
        }
        if roman {
            for (index, word) in [
                "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII",
            ]
            .iter()
            .enumerate()
            {
                let mut cursor = *self;
                if cursor.literal(word.as_bytes()).is_some()
                    && best.is_none_or(|(end, _)| cursor.pos > end)
                {
                    best = Some((cursor.pos, index as i64 + 1));
                }
            }
        }
        let (end, month) = best?;
        self.pos = end;
        Some(month)
    }
    pub fn meridian(&mut self, hour: i64) -> Option<i64> {
        let pm = self.one(b"aApP")?.eq_ignore_ascii_case(&b'p');
        self.literal(b".");
        self.one(b"mM")?;
        self.literal(b".");
        match self.peek() {
            None => self.pos += 1, // The scanner includes its padded NUL in the token.
            Some(b'\0' | b'\t' | b' ') => self.pos += 1,
            _ => return None,
        }
        Some(
            hour + if pm && hour != 12 {
                12
            } else if !pm && hour == 12 {
                -12
            } else {
                0
            },
        )
    }
    pub fn fraction(&mut self, separators: &[u8]) -> Option<i64> {
        self.one(separators)?;
        let start = self.pos;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.pos += 1;
        }
        if start == self.pos {
            return None;
        }
        // timelib converts the whole digit sequence with strtod, then scales and truncates.
        let digits = &self.input[start..self.pos];
        let value = std::str::from_utf8(digits).ok()?.parse::<f64>().ok()?;
        let result = value * 10f64.powf(6.0 - digits.len() as f64);
        Some(if result.is_finite() || cfg!(target_arch = "aarch64") {
            result as i64
        } else {
            i64::MIN
        })
    }
}

pub fn offer(best: &mut Option<Candidate>, rank: usize, cursor: Cursor<'_>, action: Action) {
    if best
        .as_ref()
        .is_none_or(|b| cursor.pos > b.len || (cursor.pos == b.len && rank < b.rank))
    {
        *best = Some(Candidate {
            len: cursor.pos,
            rank,
            action,
        });
    }
}
