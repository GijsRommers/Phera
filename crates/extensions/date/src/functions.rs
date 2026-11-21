//! Procedural dates: `getdate(0)` returns local components plus the original timestamp.

use std::time::{SystemTime, UNIX_EPOCH};

use phera_extension_api::{ArrayKey, Context, NativeResult, PhpArray, Value, args};

use crate::calendar::{Civil, month_days};
use crate::format::{DAYS, DateFormat, MONTHS};
use crate::{resolve, zone};

pub fn now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(error) => {
            -(error.duration().as_secs() as i64) - i64::from(error.duration().subsec_nanos() > 0)
        }
    }
}

pub fn date(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    formatted(context, arguments, false)
}

pub fn gmdate(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    formatted(context, arguments, true)
}

fn formatted(context: &mut dyn Context, arguments: &[Value], gmt: bool) -> NativeResult {
    let timestamp = args::int(arguments, 1).unwrap_or_else(now);
    let zone = if gmt {
        zone::Zone::default()
    } else {
        zone::current(context)
    };
    Ok(Value::from(
        DateFormat::new(timestamp, &zone, gmt).render(args::bytes(arguments, 0)),
    ))
}

pub fn idate(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let format = args::bytes(arguments, 0);
    if format.len() != 1 {
        context.warning("idate(): idate format is one char".into());
        return Ok(Value::Bool(false));
    }
    let timestamp = args::int(arguments, 1).unwrap_or_else(now);
    let date = DateFormat::new(timestamp, &zone::current(context), false);
    let value = match format[0] {
        b'd' | b'j' | b'N' | b'w' | b'z' | b'W' | b'm' | b'n' | b't' | b'L' | b'y' | b'B'
        | b'g' | b'h' | b'H' | b'G' | b'i' | b's' | b'I' | b'Z' => {
            std::str::from_utf8(&date.render(format))
                .ok()
                .and_then(|text| text.parse().ok())
                .unwrap_or_default()
        }
        b'Y' => i64::from(date.civil.year as i32),
        b'o' => i64::from(date.civil.iso_week().0 as i32),
        b'U' => i64::from(timestamp as i32),
        _ => {
            context.warning("idate(): Unrecognized date format token".into());
            return Ok(Value::Bool(false));
        }
    };
    if value == -1 {
        context.warning("idate(): Unrecognized date format token".into());
        return Ok(Value::Bool(false));
    }
    Ok(Value::Int(value))
}

pub fn getdate(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let timestamp = args::int(arguments, 0).unwrap_or_else(now);
    let (civil, _) = zone::current(context).local(timestamp);
    let mut result = PhpArray::new();
    for (name, value) in [
        ("seconds", civil.second),
        ("minutes", civil.minute),
        ("hours", civil.hour),
        ("mday", civil.day),
        ("wday", civil.weekday()),
        ("mon", civil.month),
        ("year", civil.year),
        ("yday", civil.yearday()),
    ] {
        result.insert(ArrayKey::str(name), Value::Int(value));
    }
    result.insert(
        ArrayKey::str("weekday"),
        Value::from(DAYS[civil.weekday() as usize]),
    );
    result.insert(
        ArrayKey::str("month"),
        Value::from(MONTHS[(civil.month - 1) as usize]),
    );
    result.insert(ArrayKey::Int(0), Value::Int(timestamp));
    Ok(Value::from(result))
}

pub fn localtime(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let timestamp = args::int(arguments, 0).unwrap_or_else(now);
    let (civil, offset) = zone::current(context).local(timestamp);
    let associative = args::bool(arguments, 1).unwrap_or(false);
    let mut result = PhpArray::new();
    for (index, (name, value)) in [
        ("tm_sec", civil.second),
        ("tm_min", civil.minute),
        ("tm_hour", civil.hour),
        ("tm_mday", civil.day),
        ("tm_mon", civil.month - 1),
        ("tm_year", civil.year - 1900),
        ("tm_wday", civil.weekday()),
        ("tm_yday", civil.yearday()),
        ("tm_isdst", i64::from(offset.daylight)),
    ]
    .into_iter()
    .enumerate()
    {
        let key = if associative {
            ArrayKey::str(name)
        } else {
            ArrayKey::Int(index as i64)
        };
        result.insert(key, Value::Int(value));
    }
    Ok(Value::from(result))
}

pub fn checkdate(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let month = args::int(arguments, 0).unwrap_or_default();
    let day = args::int(arguments, 1).unwrap_or_default();
    let year = args::int(arguments, 2).unwrap_or_default();
    Ok(Value::Bool(
        (1..=32767).contains(&year)
            && (1..=12).contains(&month)
            && (1..=month_days(year, month)).contains(&day),
    ))
}

pub fn mktime(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    make_time(context, arguments, false)
}

pub fn gmmktime(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    make_time(context, arguments, true)
}

fn make_time(context: &mut dyn Context, arguments: &[Value], gmt: bool) -> NativeResult {
    let zone = if gmt {
        zone::Zone::default()
    } else {
        zone::current(context)
    };
    let (current, offset) = zone.local(now());
    let year = args::int(arguments, 5)
        .map(|year| match year {
            0..=69 => year + 2000,
            70..=100 => year + 1900,
            _ => year,
        })
        .unwrap_or(current.year);
    let civil = Civil {
        year,
        month: args::int(arguments, 3).unwrap_or(current.month),
        day: args::int(arguments, 4).unwrap_or(current.day),
        hour: args::int(arguments, 0).unwrap_or_default(),
        minute: args::int(arguments, 1).unwrap_or(current.minute),
        second: args::int(arguments, 2).unwrap_or(current.second),
    };
    Ok(Value::Int(resolve::local_timestamp(
        &zone,
        civil,
        Some(offset.daylight),
    )))
}
