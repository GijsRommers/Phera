//! ICU maps time-zone aliases and Windows identifiers to IANA IDs.

use icu_time::{
    TimeZone,
    zone::{WindowsParser, iana::IanaParserExtended},
};
use phera_ext_date::{ZoneKind, parse_timezone, timezone_region, zone::Zone};
use phera_extension_api::{Context, NativeResult, Value, args};

use super::offsets;
use crate::{date, error, locale, locale_id};

pub fn find(id: &[u8]) -> Option<(String, ZoneKind)> {
    let id = id.split(|byte| *byte == 0).next()?;
    let parser = IanaParserExtended::new();
    let parsed = parser.parse_from_utf8(id);
    if parsed.time_zone != TimeZone::UNKNOWN {
        let zone = Zone::named(parsed.normalized.as_bytes())?;
        return Some((
            std::str::from_utf8(id).ok()?.to_owned(),
            ZoneKind::Identifier(zone),
        ));
    }
    let offset = id.strip_prefix(b"GMT")?;
    let zone = parse_timezone(offset)?;
    if let ZoneKind::Fixed(seconds) = zone
        && seconds.unsigned_abs() < 86_400
    {
        return Some((
            if seconds == 0 {
                "GMT".to_owned()
            } else {
                format!("GMT{}", zone.name())
            },
            zone,
        ));
    }
    None
}

pub fn same(left: &str, right: &str) -> bool {
    let parser = IanaParserExtended::new();
    let left_id = parser.parse(left).time_zone;
    left == right || (left_id != TimeZone::UNKNOWN && left_id == parser.parse(right).time_zone)
}

pub fn enumerate(
    context: &mut dyn Context,
    values: &[Value],
    site: &str,
    typed: bool,
) -> NativeResult {
    error::clear(context);
    let (kind, region, raw) = if typed {
        let kind = args::int(values, 0).unwrap_or_default();
        if !matches!(kind, 0..=2) {
            return error::fail(context, site, 1, "Unknown zone type", Value::Bool(false));
        }
        (kind, args::bytes(values, 1), args::int(values, 2))
    } else {
        (0, args::bytes(values, 0), args::int(values, 0))
    };
    let now = super::clock_millis() / 1000;
    let parser = IanaParserExtended::new();
    let mut names: Vec<_> = parser
        .iter_all()
        .filter(|id| id.time_zone != TimeZone::UNKNOWN)
        .filter(|id| kind == 0 || id.normalized == id.canonical)
        .filter(|id| {
            let country = timezone_region(id.normalized)
                .or_else(|| timezone_region(id.canonical))
                .unwrap_or("001");
            (kind != 2 || country != "001")
                && (region.is_empty() || country.as_bytes().eq_ignore_ascii_case(region))
        })
        .filter(|id| {
            raw.is_none_or(|offset| {
                Zone::named(id.normalized.as_bytes())
                    .is_some_and(|zone| offsets(&ZoneKind::Identifier(zone), now).0 == offset)
            })
        })
        .map(|id| id.normalized)
        .collect();
    names.sort_unstable();
    crate::iterator::values(context, names.into_iter().map(Value::from))
}

#[derive(Clone, Copy)]
pub enum Op {
    Count,
    Equivalent,
    Canonical,
    Iana,
    Region,
    Version,
    Windows,
    FromWindows,
}

pub fn invoke(context: &mut dyn Context, values: &[Value], site: &str, op: Op) -> NativeResult {
    error::clear(context);
    let id = args::bytes(values, 0);
    let parser = IanaParserExtended::new();
    let parsed = parser.parse_from_utf8(id);
    let valid = parsed.time_zone != TimeZone::UNKNOWN;
    let result = match op {
        Op::Count | Op::Equivalent => {
            let mut ids = parser
                .iter_all()
                .filter(|other| valid && other.time_zone == parsed.time_zone);
            if matches!(op, Op::Count) {
                Value::Int(ids.count() as i64)
            } else {
                let index =
                    usize::try_from(args::int(values, 1).unwrap_or(0)).unwrap_or(usize::MAX);
                let id = ids.nth(index).map_or("", |id| id.normalized);
                Value::from(id)
            }
        }
        Op::Canonical if valid => {
            write_system(values, true)?;
            Value::from(legacy_canonical(parsed.canonical))
        }
        Op::Canonical => {
            if let Some((fixed, _)) = find(id) {
                write_system(values, false)?;
                Value::from(fixed)
            } else {
                return error::fail(
                    context,
                    site,
                    1,
                    "error obtaining canonical ID",
                    Value::Bool(false),
                );
            }
        }
        Op::Iana if valid => Value::from(parsed.canonical),
        Op::Region if valid => {
            let region =
                timezone_region(parsed.normalized).or_else(|| timezone_region(parsed.canonical));
            if region.is_none() {
                error::warning_code(context, -124);
            }
            Value::from(region.unwrap_or("001"))
        }
        Op::Version => Value::from("2026e"),
        Op::Windows if valid => {
            let source = include_str!("../../data/cldr-windows-zones.tsv");
            source
                .lines()
                .find_map(|row| {
                    let (name, identifiers) = row.split_once('\t')?;
                    identifiers
                        .split_ascii_whitespace()
                        .any(|id| parser.parse(id).time_zone == parsed.time_zone)
                        .then(|| Value::from(name))
                })
                .unwrap_or(Value::Bool(false))
        }
        Op::FromWindows => {
            let region = std::str::from_utf8(args::bytes(values, 1))
                .ok()
                .and_then(|text| text.parse().ok());
            let windows = WindowsParser::new();
            windows
                .parse_from_utf8(id, region)
                .or_else(|| windows.parse_from_utf8(id, None))
                .and_then(|timezone| parser.iter().find(|id| id.time_zone == timezone))
                .map_or(Value::Bool(false), |id| {
                    Value::from(legacy_canonical(id.canonical))
                })
        }
        _ => return error::fail(context, site, 1, "Unknown time zone", Value::Bool(false)),
    };
    Ok(result)
}

fn legacy_canonical(identifier: &str) -> &str {
    match identifier {
        "Asia/Kolkata" => "Asia/Calcutta",
        "Asia/Kathmandu" => "Asia/Katmandu",
        "Europe/Kyiv" => "Europe/Kiev",
        "Asia/Ho_Chi_Minh" => "Asia/Saigon",
        other => other,
    }
}

fn write_system(values: &[Value], value: bool) -> Result<(), phera_extension_api::NativeError> {
    if let Some(reference) = args::reference(values, 1) {
        *reference.try_borrow_mut().map_err(|_| {
            phera_extension_api::NativeError::error("IntlTimeZone system ID reference is borrowed")
        })? = Value::Bool(value);
    }
    Ok(())
}

pub fn display(
    context: &mut dyn Context,
    zone: &ZoneKind,
    values: &[Value],
    site: &str,
    now: i64,
    raw: i64,
    dst: i64,
) -> NativeResult {
    let style = args::int(values, 1).unwrap_or(1);
    let pattern = match style {
        0 | 6 => "z",
        1 => "zzzz",
        2 => "v",
        3 => "vvvv",
        4 => "O",
        5 => "OOOO",
        7 => "VVVV",
        _ => {
            return error::fail(context, site, 1, "Invalid display type", Value::Bool(false));
        }
    };
    let wants_dst = args::bool(values, 0).unwrap_or(false);
    let instant = if wants_dst == (dst != 0) {
        now
    } else {
        [
            now.saturating_add(15_768_000),
            now.saturating_sub(15_768_000),
        ]
        .into_iter()
        .find(|time| (offsets(zone, *time).1 != 0) == wants_dst)
        .unwrap_or(now)
    };
    let requested = locale::effective(context, args::bytes(values, 2));
    let locale = locale_id::cldr(&requested);
    let fallback = ZoneKind::Fixed((raw / 1000) as i32);
    Ok(date::render_pattern(&locale, pattern, instant, zone)
        .or_else(|| date::render_pattern(&locale, "OOOO", instant, &fallback))
        .map_or(Value::Bool(false), Value::from))
}
