//! IntlTimeZone combines ICU locale IDs with Phera's PHP date transition data.
//!
//! ```php
//! IntlTimeZone::createTimeZone("Europe/Brussels")->getRawOffset(); // 3600000
//! ```

mod ids;
mod registration;

use std::rc::Rc;

use phera_ext_date::{ZoneKind, calendar::Civil, create_timezone, read_timezone, zone};
use phera_extension_api::{Context, NativeError, NativeResult, Object, PhpArray, Registry};
use phera_extension_api::{Value, args};

use crate::error;

const CLASS: &str = "IntlTimeZone";
const UNKNOWN: &str = "Etc/Unknown";

#[derive(Clone)]
struct State {
    id: String,
    zone: ZoneKind,
    error: (i32, Option<String>),
}

pub fn register(registry: &mut Registry) {
    registration::register(registry);
}

pub fn wrap_zone(context: &mut dyn Context, zone: ZoneKind) -> NativeResult {
    let id = match zone {
        ZoneKind::Fixed(_) => format!("GMT{}", zone.name()),
        _ => zone.name(),
    };
    make(context, id, zone)
}

fn make(context: &mut dyn Context, id: String, zone: ZoneKind) -> NativeResult {
    let value = context.create_object(CLASS, PhpArray::new())?;
    if let Value::Object(object) = &value {
        let state = State {
            id,
            zone,
            error: (0, None),
        };
        *object.native_payload_mut().map_err(|_| missing())? = Some(Box::new(state));
    }
    Ok(value)
}

fn read(object: &Object) -> Result<State, NativeError> {
    let data = object.native_payload().map_err(|_| missing())?;
    data.as_deref()
        .and_then(|data| data.downcast_ref::<State>())
        .cloned()
        .ok_or_else(missing)
}

pub fn read_zone(object: &Object) -> Result<ZoneKind, NativeError> {
    Ok(read(object)?.zone)
}

#[derive(Clone, Copy)]
enum Factory {
    Default,
    Named,
    Gmt,
    Unknown,
    FromDate,
}

fn factory(context: &mut dyn Context, values: &[Value], op: Factory) -> NativeResult {
    error::clear(context);
    match op {
        Factory::Default => {
            let zone = ZoneKind::Identifier(zone::current(context));
            wrap_zone(context, zone)
        }
        Factory::Gmt => make(context, "GMT".to_owned(), ZoneKind::Fixed(0)),
        Factory::Unknown => make(context, UNKNOWN.to_owned(), ZoneKind::Fixed(0)),
        Factory::Named => {
            let id = args::bytes(values, 0);
            let (id, zone) =
                ids::find(id).unwrap_or_else(|| (UNKNOWN.to_owned(), ZoneKind::Fixed(0)));
            make(context, id, zone)
        }
        Factory::FromDate => wrap_zone(
            context,
            read_timezone(args::object(values, 0).ok_or_else(missing)?)?,
        ),
    }
}

#[derive(Clone, Copy)]
enum Op {
    Id,
    Raw,
    Savings,
    Daylight,
    Offset,
    Display,
    SameRules,
    ToDate,
    ErrorCode,
    ErrorMessage,
}

fn dispatch(
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
    let state = read(object.ok_or_else(missing)?)?;
    let value = match op {
        Op::ErrorCode => return Ok(Value::Int(i64::from(state.error.0))),
        Op::ErrorMessage => {
            let name = error::name(state.error.0);
            return Ok(Value::from(state.error.1.map_or_else(
                || name.to_owned(),
                |detail| format!("{detail}: {name}"),
            )));
        }
        other => other,
    };
    error::clear(context);
    let now = clock_millis() / 1000;
    let (raw, dst) = offsets(&state.zone, now);
    match value {
        Op::Id => Ok(Value::from(state.id)),
        Op::Raw => Ok(Value::Int(raw)),
        Op::Savings => Ok(Value::Int(current_savings(&state.zone, now, raw))),
        Op::Daylight => Ok(Value::Bool(current_savings(&state.zone, now, raw) != 0)),
        Op::Offset => {
            let seconds = (args::float(values, 0).unwrap_or(0.0) / 1000.0).floor() as i64;
            let seconds = if args::bool(values, 1).unwrap_or(false) {
                state.zone.resolve(Civil::from_timestamp(seconds), None)
            } else {
                seconds
            };
            let (raw, dst) = offsets(&state.zone, seconds);
            write_offset(values, 2, raw)?;
            write_offset(values, 3, dst)?;
            Ok(Value::Bool(true))
        }
        Op::SameRules => {
            let other = read(args::object(values, 0).ok_or_else(missing)?)?;
            let equal = ids::same(&state.id, &other.id)
                || [
                    now,
                    0,
                    now.saturating_add(15_768_000),
                    now.saturating_sub(15_768_000),
                ]
                .into_iter()
                .all(|instant| offsets(&state.zone, instant) == offsets(&other.zone, instant));
            Ok(Value::Bool(equal))
        }
        Op::ToDate if state.id == UNKNOWN => {
            error::fail(context, site, 1, "Unknown time zone", Value::Bool(false))
        }
        Op::ToDate => create_timezone(context, state.zone),
        Op::Display => ids::display(context, &state.zone, values, site, now, raw, dst),
        Op::ErrorCode | Op::ErrorMessage => unreachable!("error getters already returned"),
    }
}

pub fn clock_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map_or_else(
        |error| -(error.duration().as_millis().min(i64::MAX as u128) as i64),
        |duration| duration.as_millis().min(i64::MAX as u128) as i64,
    )
}

pub fn offsets(zone: &ZoneKind, timestamp: i64) -> (i64, i64) {
    let offset = zone.offset(timestamp);
    if !offset.daylight {
        return (i64::from(offset.seconds) * 1000, 0);
    }
    let standard = [-15_768_000, 15_768_000, -2_678_400, 2_678_400]
        .into_iter()
        .map(|delta| zone.offset(timestamp.saturating_add(delta)))
        .find(|offset| !offset.daylight)
        .map_or(offset.seconds.saturating_sub(3600), |offset| offset.seconds);
    (
        i64::from(standard) * 1000,
        (i64::from(offset.seconds) - i64::from(standard)) * 1000,
    )
}

fn current_savings(zone: &ZoneKind, timestamp: i64, raw: i64) -> i64 {
    [0, 15_768_000, -15_768_000]
        .into_iter()
        .map(|delta| zone.offset(timestamp.saturating_add(delta)))
        .find(|offset| offset.daylight)
        .map_or(0, |offset| i64::from(offset.seconds) * 1000 - raw)
}

fn write_offset(values: &[Value], index: usize, offset: i64) -> Result<(), NativeError> {
    if let Some(reference) = args::reference(values, index) {
        *reference.try_borrow_mut().map_err(|_| {
            NativeError::error("IntlTimeZone offset reference is already borrowed")
        })? = Value::Int(offset);
    }
    Ok(())
}

fn missing() -> NativeError {
    NativeError::error("Found unconstructed IntlTimeZone")
}
