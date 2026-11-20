//! IntlDateFormatter supports dates, times, patterns and timezone settings.
//!
//! ```php
//! $f = new IntlDateFormatter('fr_FR', 2, -1, 'UTC');
//! $f->format(new DateTimeImmutable('2024-08-27T12:00:00Z'));
//! ```

mod historical;
pub(super) mod input;
mod parse;
mod pattern;
mod registration;
mod relative;
mod render;
mod skeleton;
mod skeleton_fields;

use std::rc::Rc;

use icu_locale::Locale;
use phera_ext_date::{ZoneKind, zone};
use phera_extension_api::{Context, NativeError, NativeResult, Object, PhpArray, Registry};
use phera_extension_api::{Value, args};

use crate::{error, locale, locale_id, number};
use pattern::Style;
use relative::Relative;
use render::Renderer;

const CLASS: &str = "IntlDateFormatter";

pub fn register(registry: &mut Registry) {
    registration::register(registry);
    skeleton::register(registry);
}

pub fn render_pattern(
    locale: &Locale,
    pattern: &str,
    seconds: i64,
    zone: &ZoneKind,
) -> Option<String> {
    Renderer::new(locale, pattern, 1)?.format(seconds, 0, zone)
}

pub fn format_message(
    context: &mut dyn Context,
    locale: &Locale,
    timestamp: f64,
    style: &str,
    time: bool,
) -> Option<String> {
    let pattern = message_pattern(locale, style, time);
    let milliseconds = (timestamp * 1000.0).floor();
    let seconds = milliseconds.div_euclid(1000.0) as i64;
    let micros = milliseconds.rem_euclid(1000.0) as i64 * 1000;
    Renderer::new(locale, &pattern, 1)?.format(seconds, micros, &current_zone(context))
}

pub fn parse_message(
    context: &mut dyn Context,
    locale: &Locale,
    input: &str,
    style: &str,
    time: bool,
) -> Option<f64> {
    let pattern = message_pattern(locale, style, time);
    let state = Formatter {
        render: Renderer::new(locale, &pattern, 1)?,
        locale: locale.clone(),
        locale_name: locale.id.to_string(),
        date_type: -1,
        time_type: -1,
        zone: current_zone(context),
        calendar: 1,
        lenient: true,
        pattern,
        relative: None,
        error: (0, None),
    };
    let (parsed, _) = parse::fields(&state, input);
    let parse::Parsed {
        civil,
        zone,
        micros,
    } = parsed?;
    Some(historical::resolve(&zone, civil) as f64 + micros as f64 / 1_000_000.0)
}

fn message_pattern(locale: &Locale, style: &str, time: bool) -> String {
    let style_kind = match style.trim() {
        "full" => Some(Style::Full),
        "long" => Some(Style::Long),
        "" | "medium" => Some(Style::Medium),
        "short" => Some(Style::Short),
        _ => None,
    };
    match style_kind {
        Some(style) if time => pattern::select(locale, Style::None, style),
        Some(style) => pattern::select(locale, style, Style::None),
        None => style.to_owned(),
    }
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
    relative: Option<Relative>,
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
    if args::bytes(values, 0).len() > locale::MAX_LEN {
        return locale::too_long_for(context, site, object.is_some());
    }
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
    let calendar_input = args::value(values, 4);
    let timezone_input = args::value(values, 3);
    let zone = match (&*timezone_input.deref(), &*calendar_input.deref()) {
        (Value::Null, Value::Object(calendar)) if calendar.class.is_instance_of("IntlCalendar") => {
            crate::calendar::calendar_zone(calendar)?
        }
        _ => input::timezone(context, timezone_input, site, 4)?,
    };
    let Some(calendar) = input::calendar(calendar_input, site, 5)? else {
        return fail_creation(context, object, site, 1, input::CALENDAR_ERROR);
    };
    let relative = Relative::new(&modern, date_type, time_style);
    let text = match std::str::from_utf8(args::bytes(values, 5)) {
        Ok(text) => {
            if let Some(relative) = &relative {
                relative.pattern(&modern, date_style, time_style)
            } else if text.is_empty() {
                pattern::select(&modern, date_style, time_style)
            } else {
                text.to_owned()
            }
        }
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
        relative,
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
            state.zone = input::timezone(context, args::value(arguments, 0), site, argument)?;
            Value::Bool(true)
        }
        Operation::SetCalendar => {
            let input = args::value(arguments, 0);
            let Some(calendar) = input::calendar(input, site, argument)? else {
                return fail(context, state, site, 1, input::CALENDAR_ERROR);
            };
            let Some(render) = Renderer::new(&state.locale, &state.pattern, calendar) else {
                return fail(context, state, site, 3, "date formatter creation failed");
            };
            state.render = render;
            state.calendar = calendar;
            if let Value::Object(object) = &*input.deref()
                && object.class.is_instance_of("IntlCalendar")
            {
                state.zone = crate::calendar::calendar_zone(object)?;
            }
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
            if state.relative.is_some() {
                return Ok(Value::Bool(true));
            }
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
        Operation::TimeZone => return crate::timezone::wrap_zone(context, state.zone.clone()),
        Operation::CalendarObject => {
            return crate::calendar::for_formatter(
                context,
                state.locale_name.clone(),
                state.zone.clone(),
                state.calendar,
            );
        }
        Operation::ParseToCalendar => return parse::parse(context, state, arguments, site, false),
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
