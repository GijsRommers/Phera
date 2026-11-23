//! `modify('+1 month')` edits a DateTime and returns a new DateTimeImmutable.

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use super::{This, datetime_factory, instant::Instant, make, receiver, state, zone_kind::ZoneKind};
use crate::{calendar::Civil, parse, parse_format::result::ParsedZone};

pub fn format(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    format_object(receiver(this)?, args::bytes(arguments, 0))
}

pub(super) fn direct_format(
    object: &phera_extension_api::Object,
    arguments: &[Option<Value>],
) -> Option<Value> {
    let [Some(format)] = arguments else {
        return None;
    };
    let format = format.as_str_bytes()?;
    format_object(object, format).ok()
}

fn format_object(object: &phera_extension_api::Object, format: &[u8]) -> NativeResult {
    let date = Instant::read(object)?;
    Ok(date.format().render(format).into())
}

pub fn timestamp(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Int(Instant::read(receiver(this)?)?.timestamp))
}

pub fn micros(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Int(Instant::read(receiver(this)?)?.micros))
}

pub fn offset(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let date = Instant::read(receiver(this)?)?;
    Ok(Value::Int(i64::from(
        date.zone.offset(date.timestamp).seconds,
    )))
}

pub fn timezone(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    make(
        context,
        "DateTimeZone",
        Instant::read(receiver(this)?)?.zone,
    )
}

pub fn modify(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let site = if Instant::base_class(object) == "DateTime" {
        "DateTime::modify"
    } else {
        "DateTimeImmutable::modify"
    };
    modify_with_site(context, this, arguments, site)
}

pub fn modify_with_site(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
    site: &str,
) -> NativeResult {
    let object = receiver(this)?;
    let mut date = Instant::read(object)?;
    let input = args::bytes(arguments, 0);
    let mut parsed = parse::parse(input);
    datetime_factory::remember(context, &parsed.parsed);
    if let Some(error) = parsed.parsed.errors.first() {
        let message = format!("{site}(): {}", datetime_factory::diagnostic(input, error));
        if site == "date_modify" {
            context.warning(message);
            return Ok(Value::Bool(false));
        }
        return Err(NativeError::throw("DateMalformedStringException", message));
    }
    let base = date.format();
    if parsed.have_zone > 0
        && matches!(parsed.parsed.zone, Some(ParsedZone::Offset(0)))
        && parsed.parsed.fields == [1970, 1, 1, 0, 0, 0].map(Some)
        && parsed.parsed.microsecond == Some(0)
    {
        date.zone = ZoneKind::Fixed(0);
    }
    parsed.have_date = false;
    parsed.parsed.microsecond.get_or_insert(date.micros);
    date.apply(&parsed, base.civil, Some(base.offset.daylight));
    date.changed(context, object)
}

pub fn set_zone(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let mut date = Instant::read(object)?;
    let timezone = args::object(arguments, 0).expect("timezone argument");
    if let Ok(zone) = state(timezone, "DateTimeZone") {
        date.zone = zone;
    }
    date.civil_override = None;
    date.changed(context, object)
}

pub fn set_timestamp(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    let object = receiver(this)?;
    let mut date = Instant::read(object)?;
    date.timestamp = integer(arguments, 0);
    date.micros = 0;
    date.civil_override = None;
    date.changed(context, object)
}

pub fn set_micros(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let micros = integer(arguments, 0);
    if !(0..1_000_000).contains(&micros) {
        let class = Instant::base_class(object);
        return Err(NativeError::throw(
            "DateRangeError",
            format!(
                "{class}::setMicrosecond(): Argument #1 ($microsecond) must be between \
             0 and 999999, {micros} given"
            ),
        ));
    }
    let mut date = Instant::read(object)?;
    date.micros = micros;
    date.changed(context, object)
}

pub fn set_time(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let mut date = Instant::read(object)?;
    let base = date.format();
    let civil = Civil {
        hour: integer(arguments, 0),
        minute: integer(arguments, 1),
        second: integer(arguments, 2),
        ..base.civil
    };
    date.wall(
        civil,
        integer(arguments, 3),
        Some(base.offset.daylight),
        false,
    );
    date.changed(context, object)
}

pub fn set_date(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let mut date = Instant::read(object)?;
    let base = date.format();
    let civil = Civil {
        year: integer(arguments, 0),
        month: integer(arguments, 1),
        day: integer(arguments, 2),
        ..base.civil
    };
    date.wall(civil, date.micros, Some(base.offset.daylight), true);
    date.changed(context, object)
}

pub fn set_iso(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let mut date = Instant::read(object)?;
    let base = date.format();
    let january = Civil {
        year: integer(arguments, 0),
        month: 1,
        day: 4,
        ..base.civil
    };
    let monday = 4 - (january.weekday() + 6) % 7;
    let week = integer(arguments, 1);
    let weekday = args::int(arguments, 2).unwrap_or(1);
    let day = monday
        .wrapping_add(week.wrapping_sub(1).wrapping_mul(7))
        .wrapping_add(weekday.wrapping_sub(1));
    date.wall(
        Civil { day, ..january },
        date.micros,
        Some(base.offset.daylight),
        true,
    );
    date.changed(context, object)
}

fn integer(arguments: &[Value], index: usize) -> i64 {
    args::int(arguments, index).unwrap_or(0)
}
