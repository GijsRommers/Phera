//! Reading one `%` specifier: `%[argnum$][flags][width][.precision]conversion`,
//! where width and precision may be `*` to take them from the values.

use phera_extension_api::{NativeError, Value};

use super::value::{Alignment, PrecisionSource, Sign, Specifier};

pub(super) struct Parser<'a> {
    pub(super) format: &'a [u8],
    pub(super) values: &'a [Value],
    /// The next byte of the format to read.
    pub(super) cursor: usize,
    /// The next value to take when a specifier names none (`%2$s` does).
    next: usize,
    /// How many values the format needs, once it has asked for one that is
    /// not there; formatting goes on to report errors in PHP's order.
    pub(super) needed: usize,
}

/// A number in a specifier that may also be `*`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Width,
    Precision,
}

impl Field {
    /// How PHP's error messages name the field.
    fn label(self) -> &'static str {
        match self {
            Field::Width => "Width",
            Field::Precision => "Precision",
        }
    }
}

impl<'a> Parser<'a> {
    pub(super) fn new(format: &'a [u8], values: &'a [Value]) -> Self {
        Self {
            format,
            values,
            cursor: 0,
            next: 0,
            needed: 0,
        }
    }

    /// The specifier after a `%` and the value it formats, or `None` when
    /// that value (or a `*` width or precision) is missing.
    pub(super) fn specifier(&mut self) -> Result<Option<(Specifier, &'a Value)>, NativeError> {
        let selected = position(self.format, &mut self.cursor)?;
        let (padding, left, sign) = self.flags()?;
        let width = if self.format.get(self.cursor) == Some(&b'*') {
            match self.star(Field::Width)? {
                Some(width) => width as usize,
                None => return Ok(None),
            }
        } else {
            number(self.format, &mut self.cursor, Field::Width.label())?
        };
        let Some((precision, explicit)) = self.precision()? else {
            return Ok(None);
        };
        if self.format.get(self.cursor) == Some(&b'l') {
            self.cursor += 1;
        }
        let index = selected.unwrap_or_else(|| {
            let index = self.next;
            self.next += 1;
            index
        });
        let Some(value) = self.values.get(index) else {
            self.needed = self.needed.max(index + 1);
            return Ok(None);
        };
        let conversion = *self
            .format
            .get(self.cursor)
            .ok_or_else(|| NativeError::value_error("Missing format specifier at end of string"))?;
        self.cursor += 1;
        if precision == Some(-1) && !matches!(conversion, b'g' | b'G' | b'h' | b'H') {
            return Err(NativeError::value_error(
                "Precision -1 is only supported for %g, %G, %h and %H",
            ));
        }
        Ok(Some((
            Specifier {
                padding,
                left,
                sign,
                width,
                precision,
                explicit,
                conversion,
            },
            value,
        )))
    }

    /// The flags: `-` to align left, `+` to always show a sign, and a padding
    /// of space, `0` or any byte after `'`. A later padding flag wins.
    fn flags(&mut self) -> Result<(u8, Alignment, Sign), NativeError> {
        let mut padding = b' ';
        let mut left = Alignment::Right;
        let mut sign = Sign::NegativeOnly;
        loop {
            match self.format.get(self.cursor) {
                Some(&flag @ (b' ' | b'0')) => padding = flag,
                Some(b'-') => left = Alignment::Left,
                Some(b'+') => sign = Sign::Always,
                Some(b'\'') => {
                    self.cursor += 1;
                    padding = *self
                        .format
                        .get(self.cursor)
                        .ok_or_else(|| NativeError::value_error("Missing padding character"))?;
                }
                _ => break,
            }
            self.cursor += 1;
        }
        Ok((padding, left, sign))
    }

    /// The precision after a `.`, which may be `*`. A dot without digits is
    /// precision 0, but [`PrecisionSource::Implicit`]: `%.s` prints the whole
    /// string where `%.0s` prints nothing.
    fn precision(&mut self) -> Result<Option<(Option<i64>, PrecisionSource)>, NativeError> {
        let mut precision = None;
        let mut explicit = PrecisionSource::Implicit;
        if self.format.get(self.cursor) == Some(&b'.') {
            self.cursor += 1;
            explicit = if self
                .format
                .get(self.cursor)
                .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'*')
            {
                PrecisionSource::Explicit
            } else {
                PrecisionSource::Implicit
            };
            precision = Some(if self.format.get(self.cursor) == Some(&b'*') {
                match self.star(Field::Precision)? {
                    Some(precision) => precision,
                    None => return Ok(None),
                }
            } else {
                number(self.format, &mut self.cursor, Field::Precision.label())? as i64
            });
        }
        Ok(Some((precision, explicit)))
    }
    /// A `*` width or precision: the next value (or the one `*2$` names),
    /// which must be an int in range; `None` when it is missing.
    fn star(&mut self, field: Field) -> Result<Option<i64>, NativeError> {
        self.cursor += 1;
        let index = position(self.format, &mut self.cursor)?.unwrap_or_else(|| {
            let index = self.next;
            self.next += 1;
            index
        });
        let Some(value) = self.values.get(index) else {
            self.needed = self.needed.max(index + 1);
            return Ok(None);
        };
        let label = field.label();
        let Value::Int(value) = &*value.deref() else {
            return Err(NativeError::value_error(format!(
                "{label} must be an integer"
            )));
        };
        // A precision of -1 means "as many digits as needed" for %g and %h.
        let minimum = if field == Field::Precision { -1 } else { 0 };
        if *value < minimum || *value > i64::from(i32::MAX) {
            return Err(NativeError::value_error(format!(
                "{label} must be between {minimum} and 2147483647"
            )));
        }
        Ok(Some(*value))
    }
}

/// A decimal number in the format, which PHP limits to `i32::MAX`.
fn number(format: &[u8], cursor: &mut usize, label: &str) -> Result<usize, NativeError> {
    let mut result = 0usize;
    while let Some(byte) = format.get(*cursor).filter(|byte| byte.is_ascii_digit()) {
        result = result
            .saturating_mul(10)
            .saturating_add(usize::from(*byte - b'0'));
        *cursor += 1;
    }
    if result > i32::MAX as usize {
        return Err(NativeError::value_error(format!(
            "{label} must be between 0 and 2147483647"
        )));
    }
    Ok(result)
}

/// An argument number such as the `2` of `%2$s`, counted from 0. Naming a
/// value does not move on to the next one for later specifiers.
fn position(format: &[u8], cursor: &mut usize) -> Result<Option<usize>, NativeError> {
    let start = *cursor;
    let mut end = start;
    while format.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if end == start || format.get(end) != Some(&b'$') {
        return Ok(None);
    }
    let value = number(format, cursor, "Argument number specifier");
    *cursor = end + 1;
    match value {
        Ok(value) if value > 0 => Ok(Some(value - 1)),
        _ => Err(NativeError::value_error(
            "Argument number specifier must be greater than zero and less than 2147483647",
        )),
    }
}
