//! `strftime('%Y-%m-%d', 0)` uses the request's timezone and the C locale.

use std::{
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};

use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

use crate::{
    format::DateFormat,
    functions,
    zone::{self, Zone},
};

pub(super) fn register(registry: &mut Registry) {
    const FORMAT: &[Param] = &[
        Param::new("format", ParamType::String),
        Param::new("timestamp", ParamType::Int)
            .nullable()
            .optional(),
    ];
    const AS_FLOAT: &[Param] =
        &[Param::new("as_float", ParamType::Bool).with_default(ConstantValue::Bool(false))];
    registry.function(FunctionEntry::deprecated_sync("strftime", FORMAT, strftime));
    registry.function(FunctionEntry::deprecated_sync(
        "gmstrftime",
        FORMAT,
        gmstrftime,
    ));
    registry.function(FunctionEntry::sync("gettimeofday", AS_FLOAT, gettimeofday));
    registry.describe_function("gettimeofday", "array|float", Some("standard"));
}

fn gettimeofday(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    if args::bool(arguments, 0).unwrap_or(false) {
        return Ok(Value::Float(duration.as_secs_f64()));
    }
    let seconds = duration.as_secs() as i64;
    let offset = zone::current(context).offset(seconds);
    let mut result = PhpArray::new();
    for (key, number) in [
        ("sec", seconds),
        ("usec", i64::from(duration.subsec_micros())),
        ("minuteswest", -i64::from(offset.seconds) / 60),
        ("dsttime", i64::from(offset.daylight)),
    ] {
        result.insert(ArrayKey::str(key), Value::Int(number));
    }
    Ok(Value::from(result))
}

fn strftime(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let zone = zone::current(context);
    Ok(formatted(arguments, &zone, false))
}

fn gmstrftime(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(formatted(arguments, &Zone::default(), true))
}

fn formatted(arguments: &[Value], zone: &Zone, utc: bool) -> Value {
    let format = args::bytes(arguments, 0)
        .split(|&byte| byte == 0)
        .next()
        .unwrap_or_default();
    let timestamp = args::int(arguments, 1).unwrap_or_else(functions::now);
    let date = DateFormat::new(timestamp, zone, utc);
    let mut output = Vec::with_capacity(format.len());
    render(&date, format, &mut output);
    if output.is_empty() {
        Value::Bool(false)
    } else {
        Value::from(output)
    }
}

fn render(date: &DateFormat, mut format: &[u8], output: &mut Vec<u8>) {
    while let Some((&byte, rest)) = format.split_first() {
        format = rest;
        if byte != b'%' {
            output.push(byte);
            continue;
        }
        let mut padding = None;
        let mut upper = false;
        let mut width = 0usize;
        while let Some((&flag, after)) = format.split_first() {
            match flag {
                b'_' => padding = Some(b' '),
                b'-' => padding = Some(0),
                b'0' if width == 0 => padding = Some(b'0'),
                b'^' if !cfg!(target_vendor = "apple") => upper = true,
                b'0'..=b'9' if !cfg!(target_vendor = "apple") => {
                    width = (width * 10 + usize::from(flag - b'0')).min(4096);
                }
                b'E' | b'O' => (),
                _ => break,
            }
            format = after;
        }
        let Some((&letter, rest)) = format.split_first() else {
            output.push(b'%');
            break;
        };
        format = rest;
        let start = output.len();
        write_directive(date, letter, output);
        if padding.is_some() || width > 0 {
            let replacement = output.split_off(start);
            let significant = if padding.is_some() {
                let first = replacement
                    .iter()
                    .position(|&byte| !matches!(byte, b'0' | b' '))
                    .unwrap_or_else(|| replacement.len().saturating_sub(1));
                &replacement[first..]
            } else {
                &replacement
            };
            let pad = padding.unwrap_or(b'0');
            if pad != 0 {
                let width = width.max(replacement.len());
                output.resize(output.len() + width.saturating_sub(significant.len()), pad);
            }
            output.extend_from_slice(significant);
        }
        if upper {
            output[start..].make_ascii_uppercase();
        }
    }
}

fn write_directive(date: &DateFormat, letter: u8, output: &mut Vec<u8>) {
    let calendar = match letter {
        b'a' => b"D".as_slice(),
        b'A' => b"l",
        b'b' | b'h' => b"M",
        b'B' => b"F",
        b'd' => b"d",
        b'F' => b"Y-m-d",
        b'G' => b"o",
        b'H' => b"H",
        b'I' => b"h",
        b'm' => b"m",
        b'M' => b"i",
        b'p' => b"A",
        b'P' if !cfg!(target_vendor = "apple") => b"a",
        b'S' => b"s",
        b'T' | b'X' => b"H:i:s",
        b'R' => b"H:i",
        b'r' => b"h:i:s A",
        b'u' => b"N",
        b'V' => b"W",
        b'w' => b"w",
        b'x' | b'D' => b"m/d/y",
        b'y' => b"y",
        b'Y' => b"Y",
        b'z' => b"O",
        b'Z' => b"T",
        _ => b"",
    };
    if !calendar.is_empty() {
        output.extend(date.render(calendar));
        return;
    }
    let date_value = date.civil;
    let hour = date_value.hour;
    match letter {
        b'%' => output.push(b'%'),
        b'n' => output.push(b'\n'),
        b't' => output.push(b'\t'),
        b'c' => render(date, b"%a %b %e %T %Y", output),
        b'+' if cfg!(target_vendor = "apple") => render(date, b"%a %b %e %T %Z %Y", output),
        b'v' if cfg!(target_vendor = "apple") => render(date, b"%e-%b-%Y", output),
        b'C' => {
            let _ = write!(output, "{:02}", date_value.year.div_euclid(100));
        }
        b'e' => {
            let _ = write!(output, "{:2}", date_value.day);
        }
        b'g' => {
            let _ = write!(output, "{:02}", date_value.iso_week().0.rem_euclid(100));
        }
        b'j' => {
            let _ = write!(output, "{:03}", date_value.yearday() + 1);
        }
        b'k' => {
            let _ = write!(output, "{hour:2}");
        }
        b'l' => {
            let hour = if hour % 12 == 0 { 12 } else { hour % 12 };
            let _ = write!(output, "{hour:2}");
        }
        b's' => {
            let _ = write!(output, "{}", date.timestamp);
        }
        b'U' | b'W' => {
            let weekday = if letter == b'W' {
                (date_value.weekday() + 6) % 7
            } else {
                date_value.weekday()
            };
            let week = (date_value.yearday() + 7 - weekday) / 7;
            let _ = write!(output, "{week:02}");
        }
        _ => {
            if !cfg!(target_vendor = "apple") {
                output.push(b'%');
            }
            output.push(letter);
        }
    }
}
