//! `DateTimeImmutable::createFromFormat('!Y-m', '2024-02')` fills unset fields from epoch.

use std::time::{SystemTime, UNIX_EPOCH};

use phera_extension_api::{Context, NativeError, NativeResult, Object, Value, args};

use super::{This, instant::Instant, make, receiver, state, store, zone_kind::ZoneKind};
use crate::{
    parse::{self, state::State, token::WeekdayBehavior},
    parse_format::{self, result::Parsed},
    zone,
};

#[derive(Default)]
struct LastErrors(Option<Value>);

pub fn remember(context: &mut dyn Context, parsed: &Parsed) {
    let errors = (!parsed.errors.is_empty() || !parsed.warnings.is_empty())
        .then(|| Value::from(parsed.diagnostics()));
    context.state().get_or_default::<LastErrors>().0 = errors;
}

pub fn last_errors(context: &mut dyn Context, _: This<'_>, _: &[Value]) -> NativeResult {
    let stored = &context.state().get_or_default::<LastErrors>().0;
    Ok(stored.clone().unwrap_or(Value::Bool(false)))
}

pub fn diagnostic(input: &[u8], error: &(usize, String)) -> String {
    let byte = parse::trim(input)
        .get(error.0)
        .copied()
        .filter(|b| *b != 0)
        .unwrap_or(b' ');
    let shown = input.split(|b| *b == 0).next().unwrap_or_default();
    let (shown, character) = (String::from_utf8_lossy(shown), [byte]);
    format!(
        "Failed to parse time string ({shown}) at position {} ({}): {}",
        error.0,
        String::from_utf8_lossy(&character),
        error.1
    )
}

pub fn construct(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let input = args::bytes(arguments, 0);
    *object.native.borrow_mut() = None;
    match parse_input(
        context,
        input,
        args::object(arguments, 1).map(|o| o.as_ref()),
        None,
    )? {
        Ok(date) => {
            store(object, date);
            Ok(Value::Null)
        }
        Err(error) => Err(NativeError::throw(
            "DateMalformedStringException",
            diagnostic(input, &error),
        )),
    }
}

pub fn create(
    context: &mut dyn Context,
    arguments: &[Value],
    class: &str,
    site: &str,
    formatted: bool,
) -> NativeResult {
    let index = usize::from(formatted);
    let input = args::bytes(arguments, index);
    if formatted && input.contains(&0) {
        return Err(NativeError::value_error(format!(
            "{site}(): Argument #2 ($datetime) must not contain any null bytes"
        )));
    }
    let timezone = args::object(arguments, index + 1).map(|object| object.as_ref());
    let format = formatted.then(|| args::bytes(arguments, 0));
    match parse_input(context, input, timezone, format)? {
        Ok(date) => make(context, class, date),
        Err(_) => Ok(Value::Bool(false)),
    }
}

type FirstError = (usize, String);

pub fn parse_input(
    context: &mut dyn Context,
    input: &[u8],
    timezone: Option<&Object>,
    format: Option<&[u8]>,
) -> Result<Result<Instant, FirstError>, NativeError> {
    let timezone =
        timezone
            .map(|object| {
                state(object, "DateTimeZone").map_err(|_| NativeError::error(
            "The DateTimeZone object has not been correctly initialized by its constructor",
        ))
            })
            .transpose();
    parse_in_zone(context, input, timezone, format)
}

pub fn parse_in_zone(
    context: &mut dyn Context,
    input: &[u8],
    timezone: Result<Option<ZoneKind>, NativeError>,
    format: Option<&[u8]>,
) -> Result<Result<Instant, FirstError>, NativeError> {
    let input = if input.is_empty() && format.is_none() {
        b"now"
    } else {
        input
    };
    let parsed = if let Some(format) = format {
        let parsed = parse_format::parse(format, input);
        State {
            weekday: parsed.weekday,
            behavior: WeekdayBehavior::OnOrAfter,
            parsed,
            ..State::default()
        }
    } else {
        parse::parse(input)
    };
    remember(context, &parsed.parsed);
    if let Some(error) = parsed.parsed.errors.first() {
        return Ok(Err(error.clone()));
    }
    let explicit = parsed.parsed.zone.as_ref().and_then(ZoneKind::parsed);
    let base = if let Some(zone) = timezone? {
        zone
    } else if let Some(zone @ ZoneKind::Identifier(_)) = &explicit {
        zone.clone()
    } else {
        ZoneKind::Identifier(zone::current(context))
    };
    Ok(Ok(resolve(
        parsed,
        base,
        explicit,
        input == b"now" && format.is_none(),
    )))
}

fn resolve(mut parsed: State, base: ZoneKind, explicit: Option<ZoneKind>, now: bool) -> Instant {
    let clock = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let base_time = base.format(clock.as_secs() as i64, i64::from(clock.subsec_micros()));
    let daylight = matches!(explicit, Some(ZoneKind::Identifier(_))).then_some(true);
    let mut date = Instant::new(
        base_time.timestamp,
        i64::from(base_time.microsecond),
        explicit.unwrap_or(base),
    );
    if !now {
        let has_fields = parsed.parsed.fields.iter().any(Option::is_some);
        let default = if has_fields { 0 } else { date.micros };
        parsed.parsed.microsecond.get_or_insert(default);
        date.apply(&parsed, base_time.civil, daylight);
    }
    date
}

pub fn from_timestamp(
    context: &mut dyn Context,
    arguments: &[Value],
    class: &str,
    declaring: &str,
) -> NativeResult {
    let (timestamp, micros) = match arguments[0] {
        Value::Int(timestamp) => (timestamp, 0),
        Value::Float(value) if (i64::MIN as f64..-(i64::MIN as f64)).contains(&value) => {
            let fraction = (value.fract() * 1_000_000.0).round() as i64;
            (
                (value as i64).wrapping_add(fraction.div_euclid(1_000_000)),
                fraction.rem_euclid(1_000_000),
            )
        }
        Value::Float(value) => {
            let shown = php_g(value);
            let bounds = format!("{} and {}.999999", i64::MIN, i64::MAX);
            return Err(NativeError::throw(
                "DateRangeError",
                format!(
                    "{declaring}::createFromTimestamp(): Argument #1 ($timestamp) must be a finite \
                 number between {bounds}, {shown} given"
                ),
            ));
        }
        _ => {
            return Err(NativeError::type_error(
                "Expected an int or float timestamp",
            ));
        }
    };
    make(
        context,
        class,
        Instant::new(timestamp, micros, ZoneKind::Fixed(0)),
    )
}

fn php_g(value: f64) -> String {
    if value.is_nan() {
        return "NAN".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-INF"
        } else {
            "INF"
        }
        .into();
    }
    let formatted = format!("{value:.5e}");
    let (mantissa, exponent) = formatted.split_once('e').expect("scientific notation");
    let exponent: i32 = exponent.parse().expect("numeric exponent");
    let trimmed = mantissa.trim_end_matches('0').trim_end_matches('.');
    let decimal = if trimmed.contains('.') { "" } else { ".0" };
    format!("{trimmed}{decimal}e{exponent:+03}")
}

pub fn from_object(context: &mut dyn Context, arguments: &[Value], class: &str) -> NativeResult {
    let source = args::object(arguments, 0).expect("date object");
    make(context, class, Instant::read(source)?)
}
