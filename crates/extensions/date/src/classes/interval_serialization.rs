//! `$interval->__serialize()` preserves the original string for relative intervals.

use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, args};

use crate::parse::token::{Special, WeekdayBehavior};

use super::{
    This,
    interval::{self, Arithmetic, Interval},
    receiver,
};

pub fn serialize(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let interval = Interval::read(object)?;
    let mut properties = interval::serialization(&interval);
    for (key, value) in object.to_array().iter() {
        if properties.get(&key).is_none() && !internal(&key) {
            properties.insert(key, value.clone());
        }
    }
    Ok(properties.into())
}

pub fn unserialize(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let data = args::array(arguments, 0).expect("serialization data");
    restore(data)?.store(object);
    for (key, value) in data.iter() {
        if matches!(&key, ArrayKey::String(_)) && !internal(&key) && !value.is_reference() {
            object.set(key, value.clone());
        }
    }
    Ok(Value::Null)
}

pub fn set_state(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    restore(args::array(arguments, 0).expect("serialization data"))?.make(context)
}

fn restore(data: &PhpArray) -> Result<Interval, NativeError> {
    let get = |key| data.get(&ArrayKey::str(key));
    if let Some(Value::String(source)) = get("date_string") {
        return interval::relative(source.as_bytes(), false).map_err(|message| {
            NativeError::error(message.replacen("): ", ") while unserializing: ", 1))
        });
    }
    let number = |key, default| {
        get(key)
            .filter(|value| {
                !matches!(
                    value,
                    Value::Array(_) | Value::Object(_) | Value::Resource(_)
                )
            })
            .map_or(default, interval::integer)
    };
    let mut fields = [0; 7];
    for (target, field) in fields.iter_mut().zip(["y", "m", "d", "h", "i", "s"]) {
        *target = number(field, -1);
    }
    fields[6] = get("f").map_or(0, interval::micros);
    let arithmetic = if number("civil_or_wall", 0) == 2 {
        Arithmetic::ElapsedClock
    } else {
        Arithmetic::Civil
    };
    let mut interval = Interval::new(fields, arithmetic);
    interval.invert = number("invert", 0);
    interval.days = if matches!(get("days"), Some(Value::Bool(false))) {
        None
    } else {
        Some(number("days", -1))
    };
    if number("have_weekday_relative", 0) != 0 {
        interval.relative.weekday = Some(number("weekday", -1));
        interval.relative.behavior = match number("weekday_behavior", -1) {
            2 => WeekdayBehavior::ThisWeek,
            1.. => WeekdayBehavior::OnOrAfter,
            _ => WeekdayBehavior::After,
        };
    }
    interval.relative.month_edge = match number("first_last_day_of", -1) {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    };
    if number("have_special_relative", 0) != 0 {
        interval.relative.special = match number("special_type", 0) {
            1 => Special::Weekdays(number("special_amount", -1)),
            2 => Special::FirstWeekday,
            3 => Special::LastWeekday,
            _ => Special::None,
        };
    }
    Ok(interval)
}

fn internal(key: &ArrayKey) -> bool {
    matches!(key, ArrayKey::String(key) if matches!(key.as_bytes(),
        b"y" | b"m" | b"d" | b"h" | b"i" | b"s" | b"f" | b"invert" | b"days"
            | b"from_string" | b"date_string"))
}
