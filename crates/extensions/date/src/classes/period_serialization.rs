//! `$period->__serialize()` snapshots dates and interval state independently of its callers.

use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, args};

use super::receiver;
use super::{
    This,
    instant::{INTERFACE, Instant},
    interval::Interval,
    period::Period,
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
    let mut data = period.properties(context)?;
    for (key, value) in object.to_array().iter() {
        if data.get(&key).is_none() {
            data.insert(key, value.clone());
        }
    }
    Ok(data.into())
}

pub fn unserialize(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let data = args::array(arguments, 0).expect("serialization data");
    restore(data)?.store(context, object)?;
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
    restore(args::array(arguments, 0).expect("serialization data"))?.make(context)
}

fn restore(data: &PhpArray) -> Result<Period, NativeError> {
    let invalid = || NativeError::error("Invalid serialization data for DatePeriod object");
    let get = |key| data.get(&ArrayKey::str(key));
    let date = |name, class_required| match get(name) {
        Some(Value::Object(object)) if object.class.is_instance_of(INTERFACE) && class_required => {
            Ok((
                Some(Instant::read(object).map_err(|_| invalid())?),
                object.class.name(),
            ))
        }
        Some(Value::Null) => Ok((None, "")),
        _ => Err(invalid()),
    };
    let (start, class) = date("start", true)?;
    let (end, _) = date("end", start.is_some())?;
    let (current, _) = date("current", start.is_some())?;
    let Some(Value::Object(interval)) = get("interval") else {
        return Err(invalid());
    };
    if interval.class.name() != "DateInterval" {
        return Err(invalid());
    }
    let interval = Interval::read(interval).map_err(|_| invalid())?;
    let (
        Some(Value::Int(recurrences)),
        Some(Value::Bool(include_start)),
        Some(Value::Bool(include_end)),
    ) = (
        get("recurrences"),
        get("include_start_date"),
        get("include_end_date"),
    )
    else {
        return Err(invalid());
    };
    if !(0..=i64::from(i32::MAX)).contains(recurrences) {
        return Err(invalid());
    }
    Ok(Period {
        start,
        end,
        current,
        interval,
        recurrences: *recurrences,
        include_start: *include_start,
        include_end: *include_end,
        date_class: class.into(),
    })
}
