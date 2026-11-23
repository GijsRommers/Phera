//! `$period->__serialize()` snapshots dates and interval state independently of its callers.

use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, args};

use super::receiver;
use super::{
    This,
    instant::{INTERFACE, Instant},
    interval::Interval,
    period::{CLASS, Period},
    uninitialized,
};

const PROPERTIES: &[&str] = &[
    "start",
    "end",
    "current",
    "interval",
    "recurrences",
    "include_start_date",
    "include_end_date",
];

pub fn serialize(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let period = Period::read(object)?;
    if !period.initialized {
        return Err(uninitialized(object, CLASS));
    }
    let mut data = period.properties(context)?;
    for (key, value) in object.to_array().iter() {
        if data.get(&key).is_none() {
            data.insert(key, value.clone());
        }
    }
    Ok(data.into())
}

pub fn unserialize(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let data = args::array(arguments, 0).expect("serialization data");
    let mut period = Period::read(object).unwrap_or_default();
    let restored = restore_into(data, &mut period);
    // Zend restores fields in order and retains the fields it accepted before an error.
    period.store(object);
    restored?;
    for (key, value) in data.iter() {
        if let ArrayKey::String(name) = &key
            && !PROPERTIES
                .iter()
                .any(|field| field.as_bytes() == name.as_bytes())
            && !value.is_reference()
        {
            object.set(key, value.clone());
        }
    }
    Ok(Value::Null)
}

pub fn set_state(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    let mut period = Period::default();
    restore_into(
        args::array(arguments, 0).expect("serialization data"),
        &mut period,
    )?;
    period.make(context)
}

fn restore_into(data: &PhpArray, period: &mut Period) -> Result<(), NativeError> {
    let invalid = || NativeError::error("Invalid serialization data for DatePeriod object");
    let get = |key| data.get(&ArrayKey::str(key));
    let date = |name, has_class| match get(name) {
        Some(Value::Object(object)) if object.class.is_instance_of(INTERFACE) && has_class => {
            Ok(Some((
                Instant::read(object).map_err(|_| invalid())?,
                object.as_ref(),
            )))
        }
        Some(Value::Null) => Ok(None),
        _ => Err(invalid()),
    };
    if let Some((start, class)) = date("start", true)? {
        period.start = Some(start);
        period.date_class = class.class.name().into();
        period.iteration_class = Instant::base_class(class);
    }
    if let Some((end, _)) = date("end", !period.date_class.is_empty())? {
        period.end = Some(end);
    }
    if let Some((current, _)) = date("current", !period.date_class.is_empty())? {
        period.current = Some(current);
    }
    let Some(Value::Object(interval)) = get("interval") else {
        return Err(invalid());
    };
    if interval.class.name() != "DateInterval" {
        return Err(invalid());
    }
    period.interval = Some(Interval::read(interval).map_err(|_| invalid())?);
    let Some(Value::Int(recurrences)) = get("recurrences") else {
        return Err(invalid());
    };
    if !(0..=i64::from(i32::MAX)).contains(recurrences) {
        return Err(invalid());
    }
    period.recurrences = *recurrences;
    let Some(Value::Bool(include_start)) = get("include_start_date") else {
        return Err(invalid());
    };
    period.include_start = *include_start;
    let Some(Value::Bool(include_end)) = get("include_end_date") else {
        return Err(invalid());
    };
    period.include_end = *include_end;
    period.initialized = true;
    Ok(())
}
