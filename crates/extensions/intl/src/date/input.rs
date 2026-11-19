//! Date formatting accepts timestamps, DateTime objects and local time arrays.
//!
//! ```php
//! $formatter->format(['tm_year' => 124, 'tm_mon' => 0, 'tm_mday' => 2]);
//! ```

use phera_ext_date::{DateTimeValue, ZoneKind, calendar::Civil, parse_timezone, read_timezone};
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, args};

use super::{Formatter, Style, current_zone, fail, pattern, render::Renderer};
use crate::{error, locale, locale_id, number};

pub const CALENDAR_ERROR: &str = concat!(
    "Invalid value for calendar type; it must be one of ",
    "IntlDateFormatter::TRADITIONAL (locale's default calendar) or ",
    "IntlDateFormatter::GREGORIAN. Alternatively, it can be an IntlCalendar object",
);

pub fn timezone(
    context: &mut dyn Context,
    value: &Value,
    site: &str,
    index: usize,
) -> Result<Option<ZoneKind>, NativeError> {
    match &*value.deref() {
        Value::Null => Ok(Some(current_zone(context))),
        Value::String(value) if value.as_bytes().is_empty() => Ok(Some(current_zone(context))),
        Value::String(value) => {
            let bytes = value.as_bytes();
            let wrapped = [b"GMT+".as_slice(), b"GMT-"]
                .iter()
                .any(|prefix| bytes.starts_with(prefix));
            Ok(parse_timezone(if wrapped { &bytes[3..] } else { bytes }))
        }
        Value::Object(object) if object.class.is_instance_of("DateTimeZone") => {
            read_timezone(object).map(Some)
        }
        Value::Object(object) if object.class.is_instance_of("IntlTimeZone") => {
            Err(number::unsupported("IntlTimeZone"))
        }
        other => Err(type_error(
            site,
            index,
            "timezone",
            "IntlTimeZone|DateTimeZone|string|null",
            other,
        )),
    }
}

pub fn calendar(value: &Value, site: &str, index: usize) -> Result<Option<i64>, NativeError> {
    match &*value.deref() {
        Value::Null => Ok(Some(1)),
        Value::Int(value) => Ok(matches!(value, 0 | 1).then_some(*value)),
        Value::Bool(value) => Ok(Some(i64::from(*value))),
        Value::Object(object) if object.class.is_instance_of("IntlCalendar") => {
            Err(number::unsupported("IntlCalendar"))
        }
        other => Err(type_error(
            site,
            index,
            "calendar",
            "IntlCalendar|int|null",
            other,
        )),
    }
}

fn type_error(site: &str, index: usize, name: &str, expected: &str, value: &Value) -> NativeError {
    NativeError::type_error(format!(
        "{site}(): Argument #{index} (${name}) must be of type {expected}, {} given",
        value.type_name(),
    ))
}

pub fn format(
    context: &mut dyn Context,
    state: &mut Formatter,
    value: &Value,
    site: &str,
) -> NativeResult {
    let (timestamp, micros) = match &*value.deref() {
        Value::Int(value) => (*value, 0),
        Value::Float(value) => numeric(*value),
        Value::String(value) => {
            let text = String::from_utf8_lossy(value.as_bytes());
            let Ok(value) = text.trim().parse::<f64>() else {
                return fail(
                    context,
                    state,
                    site,
                    1,
                    &format!(
                        "string '{text}' is not numeric, which would be required for it to be a valid date"
                    ),
                );
            };
            numeric(value)
        }
        Value::Object(value) if value.class.is_instance_of("DateTimeInterface") => {
            let date = DateTimeValue::read(value)?;
            (date.timestamp, date.micros)
        }
        Value::Object(value) if value.class.is_instance_of("IntlCalendar") => {
            return Err(number::unsupported("IntlCalendar"));
        }
        Value::Object(_) => {
            return fail(
                context,
                state,
                site,
                1,
                concat!(
                    "invalid object type for date/time (only IntlCalendar and DateTimeInterface permitted)",
                ),
            );
        }
        Value::Array(array) if !array.is_empty() => {
            if let Some(timestamp) = array_time(array, &state.zone) {
                (timestamp, 0)
            } else {
                return fail(context, state, site, 1, "invalid value in localtime array");
            }
        }
        Value::Array(_) => return Ok(Value::Bool(false)),
        _ => return fail(context, state, site, 1, "invalid PHP type for date"),
    };
    formatted(context, state, timestamp, micros, site)
}

fn numeric(value: f64) -> (i64, i64) {
    let seconds = value.floor();
    (seconds as i64, ((value - seconds) * 1_000_000.0) as i64)
}

fn array_time(array: &PhpArray, zone: &ZoneKind) -> Option<i64> {
    let field = |name: &str, default| match array
        .get(&ArrayKey::str(name))
        .as_ref()
        .map(|value| value.deref())
    {
        None => Some(default),
        Some(value) => match &*value {
            Value::Int(value) => Some(*value),
            _ => None,
        },
    };
    let civil = Civil {
        year: field("tm_year", 0)?.wrapping_add(1900),
        month: field("tm_mon", 0)?.wrapping_add(1),
        day: field("tm_mday", 1)?,
        hour: field("tm_hour", 0)?,
        minute: field("tm_min", 0)?,
        second: field("tm_sec", 0)?,
    };
    Some(zone.resolve(civil, None))
}

fn formatted(
    context: &mut dyn Context,
    state: &mut Formatter,
    timestamp: i64,
    micros: i64,
    site: &str,
) -> NativeResult {
    match state.render.format(timestamp, micros, &state.zone) {
        Some(output) => Ok(Value::from(output)),
        None => fail(context, state, site, 1, "Date formatting failed"),
    }
}

pub fn format_object(context: &mut dyn Context, arguments: &[Value], site: &str) -> NativeResult {
    error::clear(context);
    let Some(object) = args::object(arguments, 0) else {
        return Err(type_error(
            site,
            1,
            "datetime",
            "IntlCalendar|DateTimeInterface",
            args::value(arguments, 0),
        ));
    };
    if object.class.is_instance_of("IntlCalendar") {
        return Err(number::unsupported("IntlCalendar"));
    }
    if !object.class.is_instance_of("DateTimeInterface") {
        return Err(type_error(
            site,
            1,
            "datetime",
            "IntlCalendar|DateTimeInterface",
            &Value::Object(object.clone()),
        ));
    }
    let object = DateTimeValue::read(object)?;
    let requested = locale::effective(context, args::bytes(arguments, 2));
    let locale = locale_id::cldr(&requested);
    let style = |date, time| {
        Style::parse(date)
            .zip(Style::parse(time))
            .map(|(date, time)| pattern::select(&locale, date, time))
    };
    let text = match &*args::value(arguments, 1).deref() {
        Value::Null => style(2, 2),
        Value::Int(value) => style(*value, *value),
        Value::String(text) => std::str::from_utf8(text.as_bytes()).ok().map(str::to_owned),
        Value::Array(styles) => {
            let mut values = styles.iter();
            match (values.next(), values.next(), values.next()) {
                (Some((_, Value::Int(date))), Some((_, Value::Int(time))), None) => {
                    style(*date, *time)
                }
                _ => None,
            }
        }
        _ => None,
    };
    let Some(formatter) = text.and_then(|pattern| Renderer::new(&locale, &pattern, 1)) else {
        return error::fail(context, site, 1, "invalid format", Value::Bool(false));
    };
    let output = formatter.format(object.timestamp, object.micros, &object.zone);
    Ok(output.map_or(Value::Bool(false), Value::from))
}
