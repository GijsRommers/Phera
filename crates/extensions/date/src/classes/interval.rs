//! A DateInterval keeps numeric fields in native state, so `$interval->d = 10` affects add.

use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, Object, PhpArray, Value};
use phera_extension_api::{
    args,
    value::{NoDiagnostics, to_float, to_int},
};

use super::{This, assoc, datetime_factory, interval_diff, interval_iso};
use super::{instant::Instant, make, receiver, state, store, zone_kind::ZoneKind};
use crate::parse::{self, state::State, token::Special};

pub const CLASS: &str = "DateInterval";
pub(super) const FIELDS: [&str; 6] = ["y", "m", "d", "h", "i", "s"];
const UNSET: i64 = -9_999_999;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Arithmetic {
    Civil,
    ElapsedClock,
}

#[derive(Clone)]
pub struct Interval {
    pub relative: State,
    pub invert: i64,
    pub days: Option<i64>,
    pub arithmetic: Arithmetic,
    pub source: Option<Vec<u8>>,
}

impl Interval {
    pub fn new(fields: [i64; 7], arithmetic: Arithmetic) -> Self {
        Self {
            relative: State {
                relative: true,
                delta: fields,
                ..State::default()
            },
            invert: 0,
            days: None,
            arithmetic,
            source: None,
        }
    }

    pub fn from_difference(diff: interval_diff::Difference) -> Self {
        Self {
            invert: diff.invert,
            days: Some(diff.days),
            ..Self::new(diff.fields, Arithmetic::Civil)
        }
    }

    pub fn read(object: &Object) -> Result<Self, NativeError> {
        state(object, CLASS)
    }

    pub fn make(self, context: &mut dyn Context) -> NativeResult {
        make(context, CLASS, self)
    }

    pub fn store(self, object: &Object) {
        store(object, self);
    }

    pub fn special(&self) -> bool {
        self.relative.weekday.is_some() || self.relative.special != Special::None
    }
}

pub fn construct(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 0);
    let shown = displayed(input);
    let malformed = |reason| {
        NativeError::throw(
            "DateMalformedIntervalStringException",
            format!("{reason} ({shown})"),
        )
    };
    let iso = interval_iso::parse_iso(input).map_err(|()| malformed("Unknown or bad format"))?;
    let interval = if let Some(fields) = iso.fields {
        Interval::new(fields, Arithmetic::ElapsedClock)
    } else if let (Some(start), Some(end)) = (iso.start, iso.end) {
        let start = Instant::new(start, 0, ZoneKind::Fixed(0));
        let end = Instant::new(end, 0, ZoneKind::Fixed(0));
        let mut interval =
            Interval::from_difference(interval_diff::difference(&start, &end, false));
        interval.arithmetic = Arithmetic::ElapsedClock;
        interval
    } else {
        return Err(malformed("Failed to parse interval"));
    };
    interval.store(receiver(this)?);
    Ok(Value::Null)
}

pub fn from_string(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 0);
    match relative(input, true) {
        Ok(interval) => interval.make(context),
        Err(message) => Err(NativeError::throw(
            "DateMalformedIntervalStringException",
            message,
        )),
    }
}

pub fn from_string_function(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    match relative(args::bytes(arguments, 0), true) {
        Ok(interval) => interval.make(context),
        Err(message) => {
            context.warning(format!(
                "date_interval_create_from_date_string(): {message}"
            ));
            Ok(Value::Bool(false))
        }
    }
}

pub fn relative(input: &[u8], reject_absolute: bool) -> Result<Interval, String> {
    let parsed = parse::parse(input);
    if let Some(error) = parsed.parsed.errors.first() {
        let message = datetime_factory::diagnostic(input, error).replacen(
            "Failed to parse time string",
            "Unknown or bad format",
            1,
        );
        return Err(message);
    }
    if reject_absolute && (parsed.have_date || parsed.have_time != 0 || parsed.have_zone != 0) {
        return Err(format!(
            "String '{}' contains non-relative elements",
            displayed(input)
        ));
    }
    let relative = State {
        relative: true,
        delta: parsed.delta,
        weekday: parsed.weekday,
        behavior: parsed.behavior,
        special: parsed.special,
        month_edge: parsed.month_edge,
        ..State::default()
    };
    Ok(Interval {
        relative,
        source: Some(input.to_vec()),
        ..Interval::new([0; 7], Arithmetic::Civil)
    })
}

pub fn format(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let interval = Interval::read(receiver(this)?)?;
    let mut result = Vec::new();
    let mut escaped = false;
    for &byte in args::bytes(arguments, 0) {
        if escaped {
            format_code(&mut result, &interval, byte);
            escaped = false;
        } else if byte == b'%' {
            escaped = true;
        } else {
            result.push(byte);
        }
    }
    Ok(result.into())
}

fn format_code(result: &mut Vec<u8>, interval: &Interval, code: u8) {
    let fields = interval.relative.delta;
    let value = match code {
        b'Y' | b'y' => i64::from(fields[0] as i32),
        b'M' | b'm' => i64::from(fields[1] as i32),
        b'D' | b'd' => i64::from(fields[2] as i32),
        b'H' | b'h' => i64::from(fields[3] as i32),
        b'I' | b'i' => i64::from(fields[4] as i32),
        b'S' | b's' => fields[5],
        b'F' | b'f' => fields[6],
        b'a' => {
            let days = interval
                .days
                .filter(|days| *days as i32 != UNSET as i32)
                .map_or_else(|| "(unknown)".to_owned(), |days| (days as i32).to_string());
            result.extend_from_slice(days.as_bytes());
            return;
        }
        b'%' => return result.push(b'%'),
        b'r' if interval.invert == 0 => return,
        b'r' | b'R' => return result.push(if interval.invert == 0 { b'+' } else { b'-' }),
        _ => return result.extend_from_slice(&[b'%', code]),
    };
    let width = match code {
        b'F' => 6,
        code if code.is_ascii_uppercase() => 2,
        _ => 0,
    };
    result.extend_from_slice(format!("{value:0width$}").as_bytes());
}

pub fn serialization(interval: &Interval) -> PhpArray {
    if let Some(input) = &interval.source {
        return assoc([
            ("from_string", Value::Bool(true)),
            ("date_string", input.clone().into()),
        ]);
    }
    let mut properties = PhpArray::new();
    for (key, value) in FIELDS.into_iter().zip(interval.relative.delta) {
        properties.insert(ArrayKey::str(key), Value::Int(value));
    }
    for (key, value) in [
        (
            "f",
            Value::Float(interval.relative.delta[6] as f64 / 1_000_000.0),
        ),
        ("invert", Value::Int(interval.invert)),
        (
            "days",
            interval.days.map_or(Value::Bool(false), optional_integer),
        ),
        ("from_string", Value::Bool(false)),
    ] {
        properties.insert(ArrayKey::str(key), value);
    }
    properties
}

pub fn integer(value: &Value) -> i64 {
    to_int(value, &mut NoDiagnostics).unwrap_or_default()
}

pub fn micros(value: &Value) -> i64 {
    fraction_micros(to_float(value, &mut NoDiagnostics).unwrap_or_default())
}

pub(super) fn fraction_micros(value: f64) -> i64 {
    let value = value * 1_000_000.0;
    if value.is_finite() {
        let period = 18_446_744_073_709_551_616.0;
        let truncated = value.trunc() % period;
        if truncated >= -(i64::MIN as f64) {
            (truncated as u64) as i64
        } else if truncated < i64::MIN as f64 {
            ((-truncated) as u64).wrapping_neg() as i64
        } else {
            truncated as i64
        }
    } else {
        0
    }
}

pub(super) fn optional_integer(value: i64) -> Value {
    if value == UNSET {
        Value::Bool(false)
    } else {
        Value::Int(value)
    }
}

fn displayed(input: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(input.split(|byte| *byte == 0).next().unwrap_or_default())
}
