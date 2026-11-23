//! `(new DateTimeZone("+02:00"))->getLocation()` returns false for fixed offsets.

use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, Object, Value, args};

use super::{This, assoc, catalog::Location, instant::Instant, make, receiver, state, store};
use super::{transition_list, zone_kind::ZoneKind};

const CLASS: &str = "DateTimeZone";

pub(super) fn parse(input: &[u8]) -> Result<ZoneKind, String> {
    let mut end = 0;
    let (parsed, found) = crate::parse_format::zone::parse(input, &mut end);
    if let Some(zone) = ZoneKind::parsed(&parsed) {
        if matches!(zone, ZoneKind::Fixed(seconds) if seconds.unsigned_abs() >= 360_000) {
            return Err(format!(
                "Timezone offset is out of range ({})",
                String::from_utf8_lossy(input)
            ));
        }
        if found && end == input.len() {
            return Ok(zone);
        }
    }
    Err(format!(
        "Unknown or bad timezone ({})",
        String::from_utf8_lossy(input)
    ))
}

fn input_zone(input: &[u8], site: &str) -> Result<Result<ZoneKind, String>, NativeError> {
    if input.contains(&0) {
        return Err(NativeError::value_error(format!(
            "{site}(): Argument #1 ($timezone) must not contain any null bytes"
        )));
    }
    Ok(parse(input))
}

pub fn construct(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let zone =
        input_zone(args::bytes(arguments, 0), "DateTimeZone::__construct")?.map_err(|message| {
            NativeError::throw(
                "DateInvalidTimeZoneException",
                format!("DateTimeZone::__construct(): {message}"),
            )
        })?;
    store(receiver(this)?, zone);
    Ok(Value::Null)
}

pub fn open(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    match input_zone(args::bytes(arguments, 0), "timezone_open")? {
        Ok(zone) => make(context, CLASS, zone),
        Err(message) => {
            context.warning(format!("timezone_open(): {message}"));
            Ok(Value::Bool(false))
        }
    }
}

pub fn name(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(state::<ZoneKind>(receiver(this)?, CLASS)?.name().into())
}

pub fn offset(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let zone = state::<ZoneKind>(receiver(this)?, CLASS)?;
    let date = args::object(arguments, 0).expect("DateTimeInterface argument");
    let instant = Instant::read(date)?;
    Ok(Value::Int(i64::from(
        zone.offset(instant.timestamp).seconds,
    )))
}

pub fn location(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let ZoneKind::Identifier(zone) = state(receiver(this)?, CLASS)? else {
        return Ok(Value::Bool(false));
    };
    Ok(Location::find(&zone.name).map_or(Value::Bool(false), Location::value))
}

pub fn transitions(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let ZoneKind::Identifier(zone) = state(receiver(this)?, CLASS)? else {
        return Ok(Value::Bool(false));
    };
    let begin = args::int(arguments, 0).unwrap_or(i64::MIN);
    let end = args::int(arguments, 1).unwrap_or(i64::from(i32::MAX));
    transition_list::list(&zone, begin, end)
}

pub fn serialize(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let zone: ZoneKind = state(object, CLASS)?;
    let mut data = assoc([
        ("timezone_type", Value::Int(zone.kind())),
        ("timezone", zone.name().into()),
    ]);
    for (key, value) in object.to_array().iter() {
        if data.get(&key).is_none() {
            data.insert(key.clone(), value.clone());
        }
    }
    Ok(data.into())
}

pub fn unserialize(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    store(object, restore_zone(arguments)?);
    restore_custom_properties(object, arguments);
    Ok(Value::Null)
}

pub fn set_state(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    make(context, CLASS, restore_zone(arguments)?)
}

fn restore_zone(arguments: &[Value]) -> Result<ZoneKind, NativeError> {
    let invalid = || NativeError::error("Invalid serialization data for DateTimeZone object");
    let data = args::array(arguments, 0).expect("serialization array");
    if let (Some(Value::Int(1..=3)), Some(Value::String(zone))) = (
        data.get(&ArrayKey::str("timezone_type")),
        data.get(&ArrayKey::str("timezone")),
    ) && !zone.as_bytes().contains(&0)
    {
        return parse(zone.as_bytes()).map_err(|_| invalid());
    }
    Err(invalid())
}

fn restore_custom_properties(object: &Object, arguments: &[Value]) {
    let data = args::array(arguments, 0).expect("serialization array");
    for (key, value) in data.iter() {
        if let ArrayKey::String(ref name) = key
            && !matches!(name.as_bytes(), b"timezone_type" | b"timezone")
            && !value.is_reference()
        {
            object.set(key.clone(), value.clone());
        }
    }
}
