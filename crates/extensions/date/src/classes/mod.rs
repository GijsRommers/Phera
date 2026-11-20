//! Native date objects store the instant and timezone separately,
//! as in `$date->setTimezone($timezone)`.

mod abbreviations;
mod catalog;
mod datetime;
pub(crate) mod datetime_factory;
mod datetime_functions;
mod datetime_registration;
pub(super) use datetime_registration::RFC7231_FORMAT;
mod datetime_serialization;
mod instant;
mod interval;
mod interval_arithmetic;
mod interval_diff;
mod interval_iso;
mod interval_properties;
mod interval_registration;
mod interval_serialization;
mod object_hooks;
mod period;
mod period_dates;
mod period_properties;
mod period_registration;
mod period_serialization;
mod timezone;
mod timezone_registration;
mod transition_list;
mod wall_time;
mod zone_kind;

use std::rc::Rc;

use phera_extension_api::{ArrayKey, ClassEntry, Context, NativeError, NativeResult, Object};
use phera_extension_api::{PhpArray, Registry, Value};

pub use instant::Instant;
pub use zone_kind::ZoneKind;

pub fn read_timezone(object: &Object) -> Result<ZoneKind, NativeError> {
    state(object, "DateTimeZone")
}

pub fn parse_timezone(input: &[u8]) -> Option<ZoneKind> {
    timezone::parse(input).ok()
}

pub fn create_timezone(context: &mut dyn Context, zone: ZoneKind) -> NativeResult {
    make(context, "DateTimeZone", zone)
}

pub fn create_datetime(context: &mut dyn Context, instant: Instant) -> NativeResult {
    make(context, "DateTime", instant)
}

pub fn parse_datetime(
    context: &mut dyn Context,
    input: &[u8],
) -> Result<Option<Instant>, NativeError> {
    datetime_factory::parse_input(context, input, None, None).map(Result::ok)
}

pub fn timezone_region(identifier: &str) -> Option<&'static str> {
    catalog::Location::find(identifier)
        .map(catalog::Location::country)
        .filter(|country| *country != "??")
}

type This<'a> = Option<&'a Rc<Object>>;

pub fn register(registry: &mut Registry) {
    for (name, parent) in [
        ("DateError", "Error"),
        ("DateObjectError", "DateError"),
        ("DateRangeError", "DateError"),
        ("DateException", "Exception"),
        ("DateInvalidTimeZoneException", "DateException"),
        ("DateInvalidOperationException", "DateException"),
        ("DateMalformedStringException", "DateException"),
        ("DateMalformedIntervalStringException", "DateException"),
        ("DateMalformedPeriodStringException", "DateException"),
    ] {
        registry.class(ClassEntry {
            parent: Some(parent),
            ..ClassEntry::class(name)
        });
    }
    timezone_registration::register(registry);
    datetime_registration::register(registry);
    datetime_functions::register(registry);
    interval_registration::register(registry);
    period_registration::register(registry);
}

fn receiver(this: This<'_>) -> Result<&Rc<Object>, NativeError> {
    this.ok_or_else(|| NativeError::error("Missing date method receiver"))
}

fn uninitialized(object: &Object, base: &str) -> NativeError {
    let name = object.class.name();
    let displayed = if name == base {
        name.to_owned()
    } else {
        format!("{name} (inheriting {base})")
    };
    NativeError::throw(
        "DateObjectError",
        format!(
            "Object of type {displayed} has not been correctly initialized \
             by calling parent::__construct() in its constructor"
        ),
    )
}

fn state<T: Clone + 'static>(object: &Object, base: &str) -> Result<T, NativeError> {
    object
        .native
        .borrow()
        .as_ref()
        .and_then(|data| data.downcast_ref::<T>())
        .cloned()
        .ok_or_else(|| uninitialized(object, base))
}

fn store<T: 'static>(object: &Object, value: T) {
    *object.native.borrow_mut() = Some(Box::new(value));
}

fn make<T: 'static>(context: &mut dyn Context, class: &str, value: T) -> NativeResult {
    make_with_properties(context, class, value, PhpArray::new())
}

fn make_with_properties<T: 'static>(
    context: &mut dyn Context,
    class: &str,
    value: T,
    properties: PhpArray,
) -> NativeResult {
    let object = context.create_object(class, properties)?;
    if let Value::Object(object) = &object {
        store(object, value);
    }
    Ok(object)
}

fn assoc<const N: usize>(entries: [(&str, Value); N]) -> PhpArray {
    let mut array = PhpArray::new();
    for (key, value) in entries {
        array.insert(ArrayKey::str(key), value);
    }
    array
}
