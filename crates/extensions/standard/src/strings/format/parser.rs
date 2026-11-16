//! Positional and star-field parsing for PHP sprintf-family functions.

use phera_extension_api::{NativeError, Value};

use super::value::{Alignment, PrecisionSource, Sign, Specifier};

pub(super) struct Parser<'a> {
    pub(super) format: &'a [u8],
    pub(super) values: &'a [Value],
    pub(super) cursor: usize,
    next: usize,
    pub(super) needed: usize,
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

    /// Missing star arguments stop this specifier but preserve later diagnostics.
    pub(super) fn specifier(&mut self) -> Result<Option<(Specifier, &'a Value)>, NativeError> {
        let selected = position(self.format, &mut self.cursor)?;
        let (padding, left, sign) = self.flags()?;
        let width = if self.format.get(self.cursor) == Some(&b'*') {
            match star(
                self.format,
                &mut self.cursor,
                &mut self.next,
                self.values,
                "Width",
                &mut self.needed,
            )? {
                Some(width) => width as usize,
                None => return Ok(None),
            }
        } else {
            number(self.format, &mut self.cursor, "Width")?
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

    /// Later padding flags override earlier ones, including a custom quote escape.
    fn flags(&mut self) -> Result<(u8, Alignment, Sign), NativeError> {
        let mut padding = b' ';
        let mut left = Alignment::Right;
        let mut sign = Sign::NegativeOnly;
        loop {
            match self.format.get(self.cursor) {
                Some(b' ' | b'0') => {
                    padding = self.format.get(self.cursor).copied().unwrap_or(b' ')
                }
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

    /// A bare dot differs from an explicit zero for PHP strings and base conversions.
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
                match star(
                    self.format,
                    &mut self.cursor,
                    &mut self.next,
                    self.values,
                    "Precision",
                    &mut self.needed,
                )? {
                    Some(precision) => precision,
                    None => return Ok(None),
                }
            } else {
                number(self.format, &mut self.cursor, "Precision")? as i64
            });
        }
        Ok(Some((precision, explicit)))
    }
}

/// Decimal fields are bounded before allocation or narrowing to a machine int.
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

/// A positional argument does not advance the sequential argument cursor.
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

/// PHP 8.5 accepts star fields, but requires an actual integer for them.
fn star(
    format: &[u8],
    cursor: &mut usize,
    next: &mut usize,
    values: &[Value],
    label: &str,
    needed: &mut usize,
) -> Result<Option<i64>, NativeError> {
    *cursor += 1;
    let index = position(format, cursor)?.unwrap_or_else(|| {
        let index = *next;
        *next += 1;
        index
    });
    let Some(value) = values.get(index) else {
        *needed = (*needed).max(index + 1);
        return Ok(None);
    };
    let Value::Int(value) = &*value.deref() else {
        return Err(NativeError::value_error(format!(
            "{label} must be an integer"
        )));
    };
    let minimum = if label == "Precision" { -1 } else { 0 };
    if *value < minimum || *value > i64::from(i32::MAX) {
        return Err(NativeError::value_error(format!(
            "{label} must be between {minimum} and 2147483647"
        )));
    }
    Ok(Some(*value))
}
