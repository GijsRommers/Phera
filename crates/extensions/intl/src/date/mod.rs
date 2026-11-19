//! IntlDateFormatter supports dates, times, patterns and timezone settings.
//!
//! ```php
//! $f = new IntlDateFormatter('fr_FR', 2, -1, 'UTC');
//! $f->format(new DateTimeImmutable('2024-08-27T12:00:00Z'));
//! ```

mod input;
mod parse;
mod pattern;
mod registration;
mod render;

use std::rc::Rc;

use icu_locale::Locale;
use phera_ext_date::{ZoneKind, zone};
use phera_extension_api::{Context, NativeError, NativeResult, Object, PhpArray, Registry};
use phera_extension_api::{Value, args};

use crate::{error, locale, locale_id, number};
use pattern::Style;
use render::Renderer;

const CLASS: &str = "IntlDateFormatter";

pub fn register(registry: &mut Registry) {
    registration::register(registry);
}

#[derive(Clone)]
struct Formatter {
    locale: Locale,
    locale_name: String,
    date_type: i64,
    time_type: i64,
    zone: ZoneKind,
    calendar: i64,
    lenient: bool,
    pattern: String,
    render: Renderer,
    error: (i32, Option<String>),
}

type Receiver<'a> = Option<&'a Rc<Object>>;

fn init(
    context: &mut dyn Context,
    object: Receiver<'_>,
    values: &[Value],
    site: &str,
) -> NativeResult {
    error::clear(context);
    let requested = locale::effective(context, args::bytes(values, 0));
    let date_type = args::int(values, 1).unwrap_or(0);
    let time_type = args::int(values, 2).unwrap_or(0);
    let modern = locale_id::cldr(&requested);
    if modern.id.language.is_unknown() || !number::known_language(modern.id.language.as_str()) {
        return Err(NativeError::value_error(format!(
            "{site}(): Argument #1 ($locale) \"{requested}\" is invalid"
        )));
    }
    let Some((date_style, time_style)) = Style::parse(date_type).zip(Style::parse(time_type))
    else {
        return fail_creation(context, object, site, 1, "date formatter creation failed");
    };
    let Some(zone) = input::timezone(context, args::value(values, 3), site, 4)? else {
        return fail_creation(context, object, site, 1, "No such time zone");
    };
    let Some(calendar) = input::calendar(args::value(values, 4), site, 5)? else {
        return fail_creation(context, object, site, 1, input::CALENDAR_ERROR);
    };
    let text = match std::str::from_utf8(args::bytes(values, 5)) {
        Ok("") => pattern::select(&modern, date_style, time_style),
        Ok(text) => text.to_owned(),
        Err(_) => {
            return fail_creation(
                context,
                object,
                site,
                10,
                "error converting pattern to UTF-16",
            );
        }
    };
    let Some(render) = Renderer::new(&modern, &text, calendar) else {
        return fail_creation(context, object, site, 3, "date formatter creation failed");
    };
    let formatter = Formatter {
        locale: modern,
        locale_name: number::resolve_locale(&requested),
        date_type,
        time_type,
        zone,
        calendar,
        lenient: true,
        pattern: text,
        render,
        error: (0, None),
    };
    if let Some(object) = object {
        let mut payload = object.native_payload_mut().map_err(|_| borrowed())?;
        if payload.is_some() {
            return Err(NativeError::error(
                "IntlDateFormatter object is already constructed",
            ));
        }
        *payload = Some(Box::new(formatter));
        Ok(Value::Null)
    } else {
        let result = context.create_object(CLASS, PhpArray::new())?;
        if let Value::Object(object) = &result {
            *object.native_payload_mut().map_err(|_| borrowed())? = Some(Box::new(formatter));
        }
        Ok(result)
    }
}

fn fail_creation(
    context: &mut dyn Context,
    object: Receiver<'_>,
    site: &str,
    code: i32,
    detail: &str,
) -> NativeResult {
    let result = error::fail(context, site, code, detail, Value::Null);
    if object.is_some() {
        Err(NativeError::throw(
            "IntlException",
            format!("{site}(): {detail}"),
        ))
    } else {
        result
    }
}

#[derive(Clone, Copy)]
enum Operation {
    DateType,
    TimeType,
    Calendar,
    SetCalendar,
    CalendarObject,
    TimeZone,
    TimeZoneId,
    SetTimeZone,
    Pattern,
    SetPattern,
    Locale,
    SetLenient,
    Lenient,
    Format,
    Parse,
    ParseToCalendar,
    Localtime,
    ErrorCode,
    ErrorMessage,
}

fn borrowed() -> NativeError {
    NativeError::error("IntlDateFormatter is already being modified")
}

fn dispatch(
    context: &mut dyn Context,
    object: Receiver<'_>,
    values: &[Value],
    site: &str,
    operation: Operation,
) -> NativeResult {
    let (receiver, arguments) = match object {
        Some(object) => (Some(object), values),
        None => (args::object(values, 0), values.get(1..).unwrap_or_default()),
    };
    let Some(receiver) = receiver else {
        return Ok(Value::Bool(false));
    };
    let mut payload = receiver.native_payload_mut().map_err(|_| borrowed())?;
    let Some(state) = payload
        .as_mut()
        .and_then(|data| data.downcast_mut::<Formatter>())
    else {
        return error::fail(
            context,
            site,
            1,
            "Found unconstructed IntlDateFormatter",
            Value::Bool(false),
        );
    };
    match operation {
        Operation::ErrorCode => return Ok(Value::Int(i64::from(state.error.0))),
        Operation::ErrorMessage => {
            return Ok(Value::from(match &state.error.1 {
                Some(detail) => format!("{detail}: {}", error::name(state.error.0)),
                None => error::name(state.error.0).to_owned(),
            }));
        }
        _ => {
            error::clear(context);
            state.error = (0, None);
        }
    }
    let argument = usize::from(object.is_none()) + 1;
    let value = match operation {
        Operation::DateType => Value::Int(state.date_type),
        Operation::TimeType => Value::Int(state.time_type),
        Operation::Calendar => Value::Int(state.calendar),
        Operation::TimeZoneId => Value::from(state.zone.name()),
        Operation::Pattern => Value::from(state.pattern.clone()),
        Operation::Lenient => Value::Bool(state.lenient),
        Operation::SetLenient => {
            state.lenient = args::bool(arguments, 0).unwrap_or(false);
            Value::Null
        }
        Operation::SetTimeZone => {
            let Some(zone) = input::timezone(context, args::value(arguments, 0), site, argument)?
            else {
                return fail(context, state, site, 1, "No such time zone");
            };
            state.zone = zone;
            Value::Bool(true)
        }
        Operation::SetCalendar => {
            let Some(calendar) = input::calendar(args::value(arguments, 0), site, argument)? else {
                return fail(context, state, site, 1, input::CALENDAR_ERROR);
            };
            let Some(render) = Renderer::new(&state.locale, &state.pattern, calendar) else {
                return fail(context, state, site, 3, "date formatter creation failed");
            };
            state.render = render;
            state.calendar = calendar;
            Value::Bool(true)
        }
        Operation::SetPattern => {
            let Ok(pattern) = std::str::from_utf8(args::bytes(arguments, 0)) else {
                return fail(
                    context,
                    state,
                    site,
                    10,
                    "Error converting pattern to UTF-16",
                );
            };
            let Some(render) = Renderer::new(&state.locale, pattern, state.calendar) else {
                return fail(context, state, site, 3, "Error setting pattern");
            };
            state.render = render;
            state.pattern = pattern.to_owned();
            Value::Bool(true)
        }
        Operation::Locale => match args::int(arguments, 0).unwrap_or(0) {
            0 => Value::from(pattern::actual_locale(&state.locale)),
            1 => Value::from(state.locale_name.clone()),
            _ => return fail(context, state, site, 1, "Error getting locale"),
        },
        Operation::Format => return input::format(context, state, args::value(arguments, 0), site),
        Operation::Parse => return parse::parse(context, state, arguments, site, false),
        Operation::Localtime => return parse::parse(context, state, arguments, site, true),
        Operation::CalendarObject | Operation::TimeZone | Operation::ParseToCalendar => {
            return Err(number::unsupported(site));
        }
        Operation::ErrorCode | Operation::ErrorMessage => Value::Null,
    };
    Ok(value)
}

fn fail(
    context: &mut dyn Context,
    state: &mut Formatter,
    site: &str,
    code: i32,
    detail: &str,
) -> NativeResult {
    state.error = (code, Some(format!("{site}(): {detail}")));
    error::fail(context, site, code, detail, Value::Bool(false))
}

fn current_zone(context: &mut dyn Context) -> ZoneKind {
    ZoneKind::Identifier(zone::current(context))
}
