//! Date classes keep calendar data in Rust and expose PHP's property views.
//!
//! ```php
//! $copy = clone $date;
//! json_encode($copy); // date, timezone_type and timezone
//! ```

use std::cmp::Ordering;

use phera_extension_api::value::{Diagnostics, ErrorKind, OpError};
use phera_extension_api::{
    ClassHooks, Context, NativeClone, NativeComparison, NativeError, Object, PhpArray,
    PropertyPurpose, Value,
};

use super::{
    assoc,
    instant::{IMMUTABLE, Instant, MUTABLE},
    interval::{self, Interval},
    interval_properties,
    period::Period,
    period_dates, period_properties, store,
    zone_kind::ZoneKind,
};

pub(super) const DATETIME: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Copy(clone_state::<Instant>)),
    fork: Some(fork_state::<Instant>),
    compare: Some(compare_instants),
    properties: Some(datetime_view),
    ..ClassHooks::EMPTY
};

pub(super) const ZONE: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Copy(clone_state::<ZoneKind>)),
    fork: Some(fork_state::<ZoneKind>),
    compare: Some(compare_zones),
    properties: Some(timezone_view),
    ..ClassHooks::EMPTY
};

pub(super) const INTERVAL: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Copy(clone_state::<Interval>)),
    fork: Some(fork_state::<Interval>),
    compare: Some(compare_intervals),
    properties: Some(interval_view),
    property_access: Some(&interval_properties::ACCESS),
    ..ClassHooks::EMPTY
};

pub(super) const PERIOD: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Copy(clone_state::<Period>)),
    fork: Some(fork_state::<Period>),
    properties: Some(period_view),
    property_access: Some(&period_properties::ACCESS),
    iterator: Some(period_dates::cursor),
    ..ClassHooks::EMPTY
};

/// Date objects hold calendar data and no PHP value: a fork of a snapshot
/// gets a copy of it.
fn fork_state<T: Clone + 'static>(
    _object: &Object,
    state: &dyn std::any::Any,
    _fork: &mut phera_extension_api::value::Fork<'_>,
) -> Result<Box<dyn std::any::Any>, String> {
    state
        .downcast_ref::<T>()
        .map(|state| Box::new(state.clone()) as Box<dyn std::any::Any>)
        .ok_or_else(|| "its date state is not initialized".into())
}

fn clone_state<T: Clone + 'static>(
    original: &std::rc::Rc<Object>,
    copy: &Object,
) -> Result<(), NativeError> {
    let state = original
        .native
        .try_borrow()
        .map_err(|_| NativeError::error("Cannot clone date state while it is being modified"))?;
    if let Some(state) = state.as_ref().and_then(|state| state.downcast_ref::<T>()) {
        store(copy, state.clone());
    }
    Ok(())
}

fn is_date(object: &Object) -> bool {
    object.class.is_instance_of(MUTABLE) || object.class.is_instance_of(IMMUTABLE)
}

fn compare_instants(
    left: &Object,
    right: &Object,
    _: &mut dyn Diagnostics,
) -> Result<NativeComparison, OpError> {
    if !is_date(right) {
        return Ok(NativeComparison::Default);
    }
    let incomplete = |_| {
        OpError::new(
            ErrorKind::Native("DateObjectError"),
            "Trying to compare an incomplete DateTime or DateTimeImmutable object",
        )
    };
    let left = Instant::read(left).map_err(incomplete)?;
    let right = Instant::read(right).map_err(incomplete)?;
    Ok(NativeComparison::Ordered(
        (left.timestamp, left.micros).cmp(&(right.timestamp, right.micros)),
    ))
}

fn compare_zones(
    left: &Object,
    right: &Object,
    _: &mut dyn Diagnostics,
) -> Result<NativeComparison, OpError> {
    const CLASS: &str = "DateTimeZone";
    if !right.class.is_instance_of(CLASS) {
        return Ok(NativeComparison::Default);
    }
    let incomplete = |_| {
        OpError::new(
            ErrorKind::Native("DateObjectError"),
            "Trying to compare uninitialized DateTimeZone objects",
        )
    };
    let left = super::state::<ZoneKind>(left, CLASS).map_err(incomplete)?;
    let right = super::state::<ZoneKind>(right, CLASS).map_err(incomplete)?;
    if left.kind() != right.kind() {
        return Err(OpError::new(
            ErrorKind::Native("DateException"),
            "Cannot compare two different kinds of DateTimeZone objects",
        ));
    }
    Ok(if left.name() == right.name() {
        NativeComparison::Ordered(Ordering::Equal)
    } else {
        NativeComparison::Uncomparable
    })
}

fn compare_intervals(
    _: &Object,
    right: &Object,
    diagnostics: &mut dyn Diagnostics,
) -> Result<NativeComparison, OpError> {
    if !right.class.is_instance_of(interval::CLASS) {
        return Ok(NativeComparison::Default);
    }
    diagnostics.warning("Cannot compare DateInterval objects".into());
    Ok(NativeComparison::Uncomparable)
}

fn append(mut stored: PhpArray, projected: &PhpArray) -> PhpArray {
    for (key, value) in projected.iter() {
        stored.insert(key, value.clone());
    }
    stored
}

fn structured_view(purpose: PropertyPurpose) -> bool {
    matches!(
        purpose,
        PropertyPurpose::Debug
            | PropertyPurpose::Export
            | PropertyPurpose::ArrayCast
            | PropertyPurpose::Json
            | PropertyPurpose::Serialize
    )
}

fn datetime_view(
    _: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    let stored = object.to_array();
    if !structured_view(purpose) {
        return Ok(stored);
    }
    let Ok(date) = Instant::read(object) else {
        return Ok(stored);
    };
    Ok(append(
        stored,
        &assoc([
            ("date", date.format().render(b"x-m-d H:i:s.u").into()),
            ("timezone_type", Value::Int(date.zone.kind())),
            ("timezone", date.zone.name().into()),
        ]),
    ))
}

fn timezone_view(
    _: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    let stored = object.to_array();
    if !structured_view(purpose) {
        return Ok(stored);
    }
    let Ok(zone) = super::state::<ZoneKind>(object, "DateTimeZone") else {
        return Ok(stored);
    };
    Ok(append(
        stored,
        &assoc([
            ("timezone_type", Value::Int(zone.kind())),
            ("timezone", zone.name().into()),
        ]),
    ))
}

fn interval_view(
    _: &mut dyn Context,
    object: &Object,
    _: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    let Ok(interval) = Interval::read(object) else {
        return Ok(object.to_array());
    };
    let projected = interval::serialization(&interval);
    // PHP exposes these properties for subsequent reads once it has enumerated them.
    object
        .update(|stored| {
            for (key, value) in projected.iter() {
                stored.insert(key, value.clone());
            }
            stored.to_array()
        })
        .ok_or_else(|| {
            NativeError::error(
                "Cannot expose DateInterval properties while they are being modified",
            )
        })
}

fn period_view(
    context: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    let stored = object.to_array();
    if purpose == PropertyPurpose::Comparison {
        return Ok(stored);
    }
    match Period::read(object) {
        Ok(period) if period.initialized => Ok(append(stored, &period.properties(context)?)),
        _ => Ok(stored),
    }
}
