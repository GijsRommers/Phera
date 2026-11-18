//! Converting one value for its specifier, and padding it to the width.

use phera_extension_api::value::{float_repr, to_float, to_int, to_php_string};
use phera_extension_api::{Context, NativeError, Value};

use super::super::{decimal, reserve};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Alignment {
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Sign {
    Always,
    NegativeOnly,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PrecisionSource {
    Implicit,
    Explicit,
}

#[derive(Clone, Copy)]
pub(super) struct Specifier {
    pub(super) padding: u8,
    pub(super) left: Alignment,
    pub(super) sign: Sign,
    pub(super) width: usize,
    pub(super) precision: Option<i64>,
    pub(super) explicit: PrecisionSource,
    pub(super) conversion: u8,
}

impl Specifier {
    /// Characters, percent signs and non-finite floats bypass PHP field padding.
    pub(super) fn convert(
        &self,
        context: &mut dyn Context,
        value: &Value,
        name: &str,
        result: &mut Vec<u8>,
    ) -> Result<Option<Vec<u8>>, NativeError> {
        let Self {
            precision,
            explicit,
            conversion,
            ..
        } = *self;
        let text = match conversion {
            b's' => {
                let mut text = to_php_string(value, context.diagnostics())?
                    .as_bytes()
                    .to_vec();
                if explicit == PrecisionSource::Explicit {
                    text.truncate(precision.unwrap_or(0) as usize);
                }
                text
            }
            b'd' => self.signed_integer(context, value)?,
            b'u' => (to_int(value, context.diagnostics())? as u64)
                .to_string()
                .into_bytes(),
            b'b' => format!("{:b}", to_int(value, context.diagnostics())? as u64).into_bytes(),
            b'o' => format!("{:o}", to_int(value, context.diagnostics())? as u64).into_bytes(),
            b'x' => format!("{:x}", to_int(value, context.diagnostics())? as u64).into_bytes(),
            b'X' => format!("{:X}", to_int(value, context.diagnostics())? as u64).into_bytes(),
            b'c' => {
                result.push(to_int(value, context.diagnostics())? as u8);
                return Ok(None);
            }
            b'%' => {
                result.push(b'%');
                return Ok(None);
            }
            b'e' | b'E' | b'f' | b'F' | b'g' | b'G' | b'h' | b'H' => {
                return self.convert_float(context, value, name, result);
            }
            0 => return Err(NativeError::value_error("Unknown format specifier \"")),
            _ => {
                return Err(NativeError::value_error(format!(
                    "Unknown format specifier \"{}\"",
                    char::from(conversion)
                )));
            }
        };
        Ok(Some(text))
    }

    /// Numeric zero padding follows the sign; left-aligned decimal integers use spaces.
    pub(super) fn append(
        &self,
        result: &mut Vec<u8>,
        mut text: Vec<u8>,
    ) -> Result<(), NativeError> {
        let Self {
            explicit,
            conversion,
            left,
            mut padding,
            width,
            ..
        } = *self;
        if explicit == PrecisionSource::Explicit && matches!(conversion, b'b' | b'o' | b'x' | b'X')
        {
            text.clear();
        }
        if left == Alignment::Left && padding == b'0' && matches!(conversion, b'd' | b'u') {
            padding = b' ';
        }
        let count = width.saturating_sub(text.len());
        reserve(result, text.len().saturating_add(count))?;
        if left == Alignment::Right
            && padding == b'0'
            && matches!(
                conversion,
                b'd' | b'e' | b'E' | b'f' | b'F' | b'g' | b'G' | b'h' | b'H'
            )
            && text.first().is_some_and(|byte| matches!(byte, b'-' | b'+'))
        {
            result.push(text.remove(0));
        }
        if left == Alignment::Right {
            result.resize(result.len() + count, padding);
        }
        result.extend_from_slice(&text);
        if left == Alignment::Left {
            result.resize(result.len() + count, padding);
        }
        Ok(())
    }

    /// PHP preserves negative zero for general formats and caps requested precision.
    fn format_finite_float(&self, number: f64, precision: i64) -> Vec<u8> {
        let Self {
            conversion, sign, ..
        } = *self;
        let mut text = if precision == -1 {
            float_repr(number.abs()).into_bytes()
        } else if matches!(conversion, b'f' | b'F') {
            decimal::fixed(number, precision as usize)
        } else {
            decimal::significant(number, precision as usize, conversion)
        };
        if number < 0.0
            || (number == 0.0
                && number.is_sign_negative()
                && matches!(conversion, b'g' | b'G' | b'h' | b'H'))
        {
            text.insert(0, b'-');
        } else if sign == Sign::Always {
            text.insert(0, b'+');
        }
        text
    }

    /// Non-finite floats bypass padding, but still emit precision notices first.
    fn convert_float(
        &self,
        context: &mut dyn Context,
        value: &Value,
        name: &str,
        result: &mut Vec<u8>,
    ) -> Result<Option<Vec<u8>>, NativeError> {
        let precision = self.precision;
        let number = to_float(value, context.diagnostics())?;
        let mut precision = precision.unwrap_or(6);
        if precision > 53 {
            context.notice(format!("{name}(): Requested precision of {precision} digits was truncated to PHP maximum of 53 digits"));
            precision = 53;
        }
        if !number.is_finite() {
            result.extend_from_slice(if number.is_nan() {
                b"NaN"
            } else if number < 0.0 {
                b"-INF"
            } else {
                b"INF"
            });
            return Ok(None);
        }
        Ok(Some(self.format_finite_float(number, precision)))
    }

    /// A plus flag adds a sign to nonnegative decimal integers only.
    fn signed_integer(
        &self,
        context: &mut dyn Context,
        value: &Value,
    ) -> Result<Vec<u8>, NativeError> {
        let number = to_int(value, context.diagnostics())?;
        Ok(if self.sign == Sign::Always && number >= 0 {
            format!("+{number}").into_bytes()
        } else {
            number.to_string().into_bytes()
        })
    }
}
