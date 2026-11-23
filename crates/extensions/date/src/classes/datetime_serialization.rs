//! `$date->__serialize()` exposes the timestamp's civil fields and timezone kind.

use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, args};

use super::{This, assoc, datetime_factory, make, receiver, store};
use super::{
    instant::{IMMUTABLE, Instant, MUTABLE},
    zone_kind::ZoneKind,
};
use crate::zone::Zone;

pub fn serialize(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let date = Instant::read(object)?;
    let mut properties = assoc([
        ("date", date.format().render(b"x-m-d H:i:s.u").into()),
        ("timezone_type", Value::Int(date.zone.kind())),
        ("timezone", date.zone.name().into()),
    ]);
    for (key, value) in object.to_array().iter() {
        if properties.get(&key).is_none() {
            properties.insert(key, value.clone());
        }
    }
    Ok(properties.into())
}

pub fn unserialize(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let data = args::array(arguments, 0).expect("serialization data");
    let date = restore(context, data, Instant::base_class(object))?;
    store(object, date);
    for (key, value) in data.iter() {
        if let Some(name) = key.as_str_bytes()
            && !matches!(name, b"date" | b"timezone" | b"timezone_type")
            && !value.is_reference()
        {
            object.set(key, value.clone());
        }
    }
    Ok(Value::Null)
}

pub fn set_mutable(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    set_state(context, arguments, MUTABLE)
}

pub fn set_immutable(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    set_state(context, arguments, IMMUTABLE)
}

fn set_state(context: &mut dyn Context, arguments: &[Value], class: &str) -> NativeResult {
    let data = args::array(arguments, 0).expect("serialization data");
    let date = restore(context, data, class)?;
    make(context, class, date)
}

fn restore(
    context: &mut dyn Context,
    data: &PhpArray,
    class: &str,
) -> Result<Instant, NativeError> {
    let invalid = || NativeError::error(format!("Invalid serialization data for {class} object"));
    let get = |key| data.get(&ArrayKey::str(key));
    let (Some(date), Some(Value::Int(kind)), Some(timezone)) = (
        get("date").and_then(Value::as_str_bytes),
        get("timezone_type"),
        get("timezone").and_then(Value::as_str_bytes),
    ) else {
        return Err(invalid());
    };
    let mut input = date.to_vec();
    let zone = match kind {
        1 | 2 => {
            input.push(b' ');
            input.extend_from_slice(timezone);
            None
        }
        3 => Some(ZoneKind::Identifier(
            Zone::named(timezone).ok_or_else(invalid)?,
        )),
        _ => return Err(invalid()),
    };
    datetime_factory::parse_in_zone(context, &input, Ok(zone), None)?.map_err(|_| invalid())
}
