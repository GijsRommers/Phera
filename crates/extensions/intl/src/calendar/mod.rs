//! Calendar objects keep a millisecond instant, a zone and locale week rules.

mod fields;
mod methods;
mod registration;

use std::rc::Rc;

use icu_calendar::{AnyCalendarKind, types::Weekday, week::WeekInformation};
use phera_ext_date::{DateTimeValue, ZoneKind, zone};
use phera_extension_api::{Context, NativeError, NativeResult, Object, PhpArray, Registry};
use phera_extension_api::{Value, args};

use crate::{date, error, locale, locale_id, timezone};

const CLASS: &str = "IntlCalendar";
const GREGORIAN: &str = "IntlGregorianCalendar";
const DEFAULT_CHANGE: f64 = -12_219_292_800_000.0;

#[derive(Clone)]
struct State {
    instant: f64,
    zone: ZoneKind,
    locale: String,
    kind: AnyCalendarKind,
    first: i64,
    minimal_days: i64,
    weekend: u8,
    lenient: bool,
    unset: u32,
    gregorian_change: f64,
    repeated_wall: i64,
    skipped_wall: i64,
    error: (i32, Option<String>),
}

impl State {
    fn new(locale: String, zone: ZoneKind, gregorian: bool, instant: f64) -> Self {
        let modern = locale_id::cldr(&locale);
        let kind = if gregorian {
            AnyCalendarKind::Gregorian
        } else {
            AnyCalendarKind::try_new(modern.clone().into()).unwrap_or(AnyCalendarKind::Gregorian)
        };
        let week = WeekInformation::try_new(modern.into()).ok();
        let first = week.map_or(1, |info| day(info.first_weekday));
        let minimal_days = match locale_id::cldr(&locale)
            .id
            .region
            .as_ref()
            .map(|r| r.as_str())
        {
            Some("GB" | "BE" | "NL" | "FR" | "DE" | "ES" | "IT" | "RU") => 4,
            _ => 1,
        };
        let weekend = week.map_or(0b1000001, |info| {
            info.weekend()
                .fold(0, |mask, weekday| mask | (1 << (day(weekday) - 1)))
        });
        Self {
            instant,
            zone,
            locale,
            kind,
            first,
            minimal_days,
            weekend,
            lenient: true,
            unset: 0,
            gregorian_change: DEFAULT_CHANGE,
            repeated_wall: 0,
            skipped_wall: 0,
            error: (0, None),
        }
    }

    fn date_type(&self) -> i64 {
        i64::from(matches!(
            self.kind,
            AnyCalendarKind::Gregorian | AnyCalendarKind::Iso
        ))
    }

    fn as_datetime(&self) -> DateTimeValue {
        let seconds = self.instant.div_euclid(1000.0) as i64;
        let micros = (self.instant.rem_euclid(1000.0) * 1000.0) as i64;
        DateTimeValue::new(seconds, micros, self.zone.clone())
    }
}

pub fn register(registry: &mut Registry) {
    registration::register(registry);
}

fn day(day: Weekday) -> i64 {
    i64::from(day as u8 % 7) + 1
}

fn update<T>(object: &Object, f: impl FnOnce(&mut State) -> T) -> Result<T, NativeError> {
    let mut data = object.native_payload_mut().map_err(|_| missing())?;
    data.as_deref_mut()
        .and_then(|data| data.downcast_mut::<State>())
        .map(f)
        .ok_or_else(missing)
}

pub fn timestamp(object: &Object) -> Result<(i64, i64), NativeError> {
    update(object, |state| {
        let date = state.as_datetime();
        (date.timestamp, date.micros)
    })
}

pub fn format_input(object: &Object) -> Result<(DateTimeValue, Option<&'static str>), NativeError> {
    update(object, |state| {
        let calendar = (state.date_type() == 0).then(|| methods::kind(state.kind));
        (state.as_datetime(), calendar)
    })
}

pub fn calendar_type(object: &Object) -> Result<i64, NativeError> {
    update(object, |state| state.date_type())
}

pub fn calendar_zone(object: &Object) -> Result<ZoneKind, NativeError> {
    update(object, |state| state.zone.clone())
}

pub fn for_formatter(
    context: &mut dyn Context,
    locale: String,
    zone: ZoneKind,
    calendar: i64,
) -> NativeResult {
    let gregorian = calendar == 1;
    let state = State::new(locale, zone, gregorian, timezone::clock_millis() as f64);
    make(context, None, state)
}

fn make(context: &mut dyn Context, object: Option<&Rc<Object>>, state: State) -> NativeResult {
    if let Some(object) = object {
        *object.native_payload_mut().map_err(|_| missing())? = Some(Box::new(state));
        return Ok(Value::Null);
    }
    let class = if state.kind == AnyCalendarKind::Gregorian {
        GREGORIAN
    } else {
        CLASS
    };
    let value = context.create_object(class, PhpArray::new())?;
    if let Value::Object(object) = &value {
        *object.native_payload_mut().map_err(|_| missing())? = Some(Box::new(state));
    }
    Ok(value)
}

#[derive(Clone, Copy)]
enum Factory {
    Instance,
    Gregorian,
    FromDate,
    Date,
    DateTime,
}

fn create(
    context: &mut dyn Context,
    object: Option<&Rc<Object>>,
    values: &[Value],
    site: &str,
    op: Factory,
) -> NativeResult {
    error::clear(context);
    let instant = timezone::clock_millis() as f64;
    let numbered = matches!(op, Factory::Date | Factory::DateTime)
        || (matches!(op, Factory::Gregorian) && matches!(args::value(values, 0), Value::Int(_)));
    if numbered {
        let zone = ZoneKind::Identifier(zone::current(context));
        let locale = locale::effective(context, b"");
        let mut state = State::new(locale, zone, true, instant);
        let civil = fields::components(values, state.as_datetime().format().civil, false);
        if !fields::set_civil(&mut state, civil, 0) {
            return Err(NativeError::value_error(format!(
                "{site}(): Invalid calendar fields"
            )));
        }
        return make(context, object, state);
    }
    let locale = locale::effective(context, args::bytes(values, 1));
    if matches!(op, Factory::FromDate) {
        let date = if let Some(object) = args::object(values, 0) {
            DateTimeValue::read(object)?
        } else {
            let Some(date) = phera_ext_date::parse_datetime(context, args::bytes(values, 0))?
            else {
                return Ok(Value::Null);
            };
            date
        };
        let instant = date.timestamp as f64 * 1000.0 + date.micros as f64 / 1000.0;
        return make(context, None, State::new(locale, date.zone, false, instant));
    }
    let zone = date::input::timezone(context, args::value(values, 0), site, 1)?;
    make(
        context,
        object,
        State::new(locale, zone, matches!(op, Factory::Gregorian), instant),
    )
}

fn missing() -> NativeError {
    NativeError::error("Found unconstructed IntlCalendar")
}
