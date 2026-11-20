//! Calendar operations share the same payload for methods and procedural calls.

use std::rc::Rc;

use icu_calendar::AnyCalendarKind;
use phera_ext_date::{calendar::leap, create_datetime};
use phera_extension_api::{Context, NativeError, NativeResult, Object, Value, args};

use super::{State, fields, missing, update};
use crate::{date, error, number, timezone, unsupported};

#[derive(Clone, Copy)]
pub enum Op {
    Get,
    Add,
    Set,
    SetDate,
    SetDateTime,
    Time,
    SetTime,
    TimeZone,
    SetTimeZone,
    ToDate,
    First,
    SetFirst,
    Minimal,
    SetMinimal,
    Weekend,
    DayType,
    WeekendTransition,
    Before,
    After,
    Equals,
    Equivalent,
    Lenient,
    SetLenient,
    Locale,
    Kind,
    Dst,
    Clear,
    IsSet,
    Minimum,
    Maximum,
    ActualMin,
    ActualMax,
    GreatestMin,
    LeastMax,
    GregorianChange,
    SetGregorianChange,
    Leap,
    ErrorCode,
    ErrorMessage,
    Roll,
    Repeated,
    Skipped,
    SetRepeated,
    SetSkipped,
}

pub fn invoke(
    context: &mut dyn Context,
    object: Option<&Rc<Object>>,
    values: &[Value],
    site: &str,
    op: Op,
) -> NativeResult {
    let (object, values) = match object {
        Some(object) => (Some(object), values),
        None => (args::object(values, 0), values.get(1..).unwrap_or_default()),
    };
    update(object.ok_or_else(missing)?, |state| {
        apply(context, state, values, site, op)
    })?
}

fn apply(
    context: &mut dyn Context,
    state: &mut State,
    values: &[Value],
    site: &str,
    op: Op,
) -> NativeResult {
    match op {
        Op::ErrorCode => return Ok(Value::Int(i64::from(state.error.0))),
        Op::ErrorMessage => {
            let name = error::name(state.error.0);
            return Ok(Value::from(state.error.1.as_ref().map_or_else(
                || name.to_owned(),
                |detail| format!("{detail}: {name}"),
            )));
        }
        _ => {}
    }
    error::clear(context);
    state.error = (0, None);
    let field = args::int(values, 0).unwrap_or_default();
    let value = match op {
        Op::Get => {
            let Some(value) = fields::read(state, field) else {
                return field_error(site);
            };
            Value::Int(value)
        }
        Op::Add | Op::Roll => {
            let amount = args::int(values, 1)
                .or_else(|| args::bool(values, 1).map(|up| if up { 1 } else { -1 }))
                .unwrap_or_default();
            if !(0..23).contains(&field) {
                return field_error(site);
            }
            let result = if matches!(op, Op::Roll) {
                let Some(low) = fields::limits(state, field, false, true) else {
                    return field_error(site);
                };
                let Some(high) = fields::limits(state, field, true, true) else {
                    return field_error(site);
                };
                let current = fields::read(state, field).unwrap_or(low);
                let new = (current - low)
                    .saturating_add(amount)
                    .rem_euclid(high - low + 1)
                    + low;
                fields::alter(state, field, new, false)
            } else {
                fields::alter(state, field, amount, true)
            };
            Value::Bool(result)
        }
        Op::Set => return set_fields(context, state, values, site, field),
        Op::SetDate | Op::SetDateTime => {
            fields::check_components(values, fields::CIVIL_FIELDS, site, 1)?;
            if !fields::date(state, values, matches!(op, Op::SetDate)) {
                return Err(NativeError::value_error(format!(
                    "{site}(): Invalid calendar fields"
                )));
            }
            Value::Null
        }
        Op::Time => Value::Float(state.instant),
        Op::SetTime => {
            let amount = args::float(values, 0).unwrap_or_default();
            if !amount.is_finite() {
                return failure(context, state, site, "Invalid time");
            }
            state.instant = amount;
            state.unset = 0;
            Value::Bool(true)
        }
        Op::TimeZone => return timezone::wrap_zone(context, state.zone.clone()),
        Op::SetTimeZone => {
            state.zone = date::input::timezone(context, args::value(values, 0), site, 1)?;
            Value::Bool(true)
        }
        Op::ToDate => return create_datetime(context, state.as_datetime()),
        Op::First => Value::Int(state.first),
        Op::SetFirst | Op::SetMinimal => {
            if matches!(op, Op::SetFirst) {
                check_weekday(field, site)?;
                state.first = field;
            } else {
                if !(1..=7).contains(&field) {
                    let argument = first_argument(site);
                    return Err(NativeError::value_error(format!(
                        "{site}(): Argument #{argument} ($days) must be between 1 and 7"
                    )));
                }
                state.minimal_days = field;
            }
            Value::Bool(true)
        }
        Op::Minimal => Value::Int(state.minimal_days),
        Op::Weekend => {
            let day = args::float(values, 0).map_or_else(
                || state.as_datetime().format().civil.weekday() + 1,
                |time| {
                    state
                        .zone
                        .format((time / 1000.0).floor() as i64, 0)
                        .civil
                        .weekday()
                        + 1
                },
            );
            Value::Bool(is_weekend(state, day))
        }
        Op::DayType | Op::WeekendTransition => {
            check_weekday(field, site)?;
            if matches!(op, Op::DayType) {
                Value::Int(i64::from(is_weekend(state, field)))
            } else {
                return failure(context, state, site, "Error getting weekend transition");
            }
        }
        Op::Before | Op::After | Op::Equals | Op::Equivalent => {
            let other = args::object(values, 0).ok_or_else(missing)?;
            let (time, zone, kind, first, lenient) = match other.native_payload() {
                Ok(data) => {
                    let state = data
                        .as_deref()
                        .and_then(|data| data.downcast_ref::<State>())
                        .ok_or_else(missing)?;
                    (
                        state.instant,
                        state.zone.name(),
                        state.kind,
                        state.first,
                        state.lenient,
                    )
                }
                Err(_) => (
                    state.instant,
                    state.zone.name(),
                    state.kind,
                    state.first,
                    state.lenient,
                ),
            };
            let compatible = state.kind == kind
                && state.first == first
                && state.zone.name() == zone
                && state.lenient == lenient;
            Value::Bool(match op {
                Op::Before => state.instant < time,
                Op::After => state.instant > time,
                Op::Equals => state.instant == time && compatible,
                _ => compatible,
            })
        }
        Op::Lenient => Value::Bool(state.lenient),
        Op::SetLenient => {
            state.lenient = args::bool(values, 0).unwrap_or(false);
            Value::Bool(true)
        }
        Op::Locale => match field {
            0 => Value::from(""),
            1 => Value::from(number::resolve_locale(&state.locale)),
            _ => return failure(context, state, site, "Error obtaining locale"),
        },
        Op::Kind => Value::from(kind(state.kind)),
        Op::Dst => Value::Bool(state.zone.offset(state.as_datetime().timestamp).daylight),
        Op::IsSet => {
            if !(0..23).contains(&field) {
                return field_error(site);
            }
            Value::Bool((state.unset & (1 << field)) == 0)
        }
        Op::Clear => {
            if args::int(values, 0).is_some() {
                if !(0..23).contains(&field) {
                    return field_error(site);
                }
                state.unset |= 1 << field;
            } else {
                state.unset = (1 << 23) - 1;
                state.instant = f64::from(-state.zone.offset(0).seconds) * 1000.0;
            }
            Value::Bool(true)
        }
        Op::Minimum
        | Op::Maximum
        | Op::ActualMin
        | Op::ActualMax
        | Op::GreatestMin
        | Op::LeastMax => {
            let maximal = matches!(op, Op::ActualMax | Op::Maximum | Op::LeastMax);
            let actual = matches!(op, Op::ActualMin | Op::ActualMax);
            let Some(result) = fields::limits(state, field, maximal, actual) else {
                return field_error(site);
            };
            Value::Int(result)
        }
        Op::GregorianChange => Value::Float(state.gregorian_change),
        Op::SetGregorianChange => {
            state.gregorian_change = args::float(values, 0).unwrap_or_default();
            Value::Bool(true)
        }
        Op::Leap => Value::Bool(if field < 1582 {
            field % 4 == 0
        } else {
            leap(field)
        }),
        Op::Repeated => Value::Int(state.repeated_wall),
        Op::Skipped => Value::Int(state.skipped_wall),
        Op::SetRepeated | Op::SetSkipped => {
            return set_wall_option(state, site, field, matches!(op, Op::SetRepeated));
        }
        Op::ErrorCode | Op::ErrorMessage => return Err(unsupported::item(site)),
    };
    Ok(value)
}

fn set_fields(
    context: &mut dyn Context,
    state: &mut State,
    values: &[Value],
    site: &str,
    field: i64,
) -> NativeResult {
    let first = first_argument(site);
    let multiple = args::int(values, 2).is_some();
    if first == 2 {
        context.deprecated(
            concat!(
                "Function intlcal_set() is deprecated since 8.4, use IntlCalendar::set(), ",
                "IntlCalendar::setDate(), or IntlCalendar::setDateTime() instead"
            )
            .to_owned(),
        );
    } else if multiple {
        context.deprecated(
            concat!(
                "Calling IntlCalendar::set() with more than 2 arguments is deprecated, ",
                "use either IntlCalendar::setDate() or IntlCalendar::setDateTime() instead"
            )
            .to_owned(),
        );
    }
    if multiple {
        if args::int(values, 3).is_some() && args::int(values, 4).is_none() {
            return Err(NativeError::argument_count_error(format!(
                "{site}() has no variant with exactly {} parameters",
                first + 3
            )));
        }
        fields::check_components(values, fields::CIVIL_FIELDS, site, first)?;
    } else {
        if !(0..23).contains(&field) {
            return invalid_field(site, "year");
        }
        fields::check_components(values, &["year", "month"], site, first)?;
    }
    Ok(Value::Bool(fields::set(state, values)))
}

fn set_wall_option(state: &mut State, site: &str, option: i64, repeated: bool) -> NativeResult {
    let max = if repeated { 1 } else { 2 };
    if !(0..=max).contains(&option) {
        let allowed = if repeated {
            "either IntlCalendar::WALLTIME_FIRST or IntlCalendar::WALLTIME_LAST"
        } else {
            concat!(
                "one of IntlCalendar::WALLTIME_FIRST, IntlCalendar::WALLTIME_LAST, ",
                "or IntlCalendar::WALLTIME_NEXT_VALID"
            )
        };
        let argument = first_argument(site);
        return Err(NativeError::value_error(format!(
            "{site}(): Argument #{argument} ($option) must be {allowed}"
        )));
    }
    if repeated {
        state.repeated_wall = option;
    } else {
        state.skipped_wall = option;
    }
    Ok(Value::Bool(true))
}

pub fn kind(kind: AnyCalendarKind) -> &'static str {
    match kind {
        AnyCalendarKind::Buddhist => "buddhist",
        AnyCalendarKind::Chinese => "chinese",
        AnyCalendarKind::Coptic => "coptic",
        AnyCalendarKind::Dangi => "dangi",
        AnyCalendarKind::Ethiopian => "ethiopic",
        AnyCalendarKind::EthiopianAmeteAlem => "ethiopic-amete-alem",
        AnyCalendarKind::Hebrew => "hebrew",
        AnyCalendarKind::Indian => "indian",
        AnyCalendarKind::HijriTabularTypeIIFriday => "islamic-civil",
        AnyCalendarKind::HijriSimulatedMecca => "islamic",
        AnyCalendarKind::HijriTabularTypeIIThursday => "islamic-tbla",
        AnyCalendarKind::HijriUmmAlQura => "islamic-umalqura",
        AnyCalendarKind::Japanese => "japanese",
        AnyCalendarKind::Julian => "julian",
        AnyCalendarKind::Persian => "persian",
        AnyCalendarKind::Roc => "roc",
        AnyCalendarKind::Iso => "iso8601",
        _ => "gregorian",
    }
}

fn field_error(site: &str) -> NativeResult {
    invalid_field(site, "field")
}

fn first_argument(site: &str) -> usize {
    1 + usize::from(site.starts_with("intlcal_") || site.starts_with("intlgregcal_"))
}

fn invalid_field(site: &str, name: &str) -> NativeResult {
    let argument = first_argument(site);
    Err(NativeError::value_error(format!(
        "{site}(): Argument #{argument} (${name}) must be a valid field"
    )))
}

fn check_weekday(day: i64, site: &str) -> Result<(), NativeError> {
    if (1..=7).contains(&day) {
        return Ok(());
    }
    let argument = first_argument(site);
    Err(NativeError::value_error(format!(
        "{site}(): Argument #{argument} ($dayOfWeek) must be a valid day of the week"
    )))
}

fn is_weekend(state: &State, day: i64) -> bool {
    (state.weekend & (1 << (day - 1))) != 0
}

fn failure(context: &mut dyn Context, state: &mut State, site: &str, detail: &str) -> NativeResult {
    state.error = (1, Some(format!("{site}(): {detail}")));
    error::fail(context, site, 1, detail, Value::Bool(false))
}
