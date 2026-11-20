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
        Op::Set => {
            if args::int(values, 2).is_none() && !(0..23).contains(&field) {
                return field_error(site);
            }
            Value::Bool(fields::set(state, values))
        }
        Op::SetDate | Op::SetDateTime => {
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
            let Some(zone) = date::input::timezone(context, args::value(values, 0), site, 1)?
            else {
                return failure(context, state, site, "No such time zone");
            };
            state.zone = zone;
            Value::Bool(true)
        }
        Op::ToDate => return create_datetime(context, state.as_datetime()),
        Op::First => Value::Int(state.first),
        Op::SetFirst | Op::SetMinimal => {
            check_weekday(field, site)?;
            if matches!(op, Op::SetFirst) {
                state.first = field;
            } else {
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
            if !(0..=2).contains(&field) {
                return failure(context, state, site, "Invalid wall time option");
            }
            if matches!(op, Op::SetRepeated) {
                state.repeated_wall = field;
            } else {
                state.skipped_wall = field;
            }
            Value::Bool(true)
        }
        Op::ErrorCode | Op::ErrorMessage => return Err(unsupported::item(site)),
    };
    Ok(value)
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
    Err(NativeError::value_error(format!(
        "{site}(): Argument #1 ($field) must be a valid field"
    )))
}

fn check_weekday(day: i64, site: &str) -> Result<(), NativeError> {
    if (1..=7).contains(&day) {
        return Ok(());
    }
    Err(NativeError::value_error(format!(
        "{site}(): Argument #1 ($dayOfWeek) must be a valid day of the week"
    )))
}

fn is_weekend(state: &State, day: i64) -> bool {
    (state.weekend & (1 << (day - 1))) != 0
}

fn failure(context: &mut dyn Context, state: &mut State, site: &str, detail: &str) -> NativeResult {
    state.error = (1, Some(format!("{site}(): {detail}")));
    error::fail(context, site, 1, detail, Value::Bool(false))
}
