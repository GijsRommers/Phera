//! `DateInterval::createFromDateString(datetime: 'next Monday')` parses relative text.

use phera_extension_api::args;
use phera_extension_api::{ClassEntry, Context, FunctionEntry, MethodEntry};
use phera_extension_api::{NativeResult, Param, ParamType, Registry, Value};

use super::{interval, interval_arithmetic, interval_serialization};

pub const INTERVAL: Param = Param::new("interval", ParamType::Class(interval::CLASS));

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        hooks: Some(&super::object_hooks::INTERVAL),
        methods: METHODS,
        ..ClassEntry::class(interval::CLASS)
    });
    registry.function(FunctionEntry::sync(
        "date_interval_create_from_date_string",
        DATETIME,
        interval::from_string_function,
    ));
    registry.function(FunctionEntry::sync(
        "date_interval_format",
        &[OBJECT, FORMAT],
        format,
    ));
}

const OBJECT: Param = Param::new("object", ParamType::Class(interval::CLASS));
const FORMAT: Param = Param::new("format", ParamType::String);
const DATETIME: &[Param] = &[Param::new("datetime", ParamType::String)];

const METHODS: &[MethodEntry] = &[
    MethodEntry::public(
        "__construct",
        &[Param::new("duration", ParamType::String)],
        interval::construct,
    ),
    MethodEntry::public("createFromDateString", DATETIME, interval::from_string).static_(),
    MethodEntry::public("format", &[FORMAT], interval::format),
    MethodEntry::public("__serialize", &[], interval_serialization::serialize),
    MethodEntry::public(
        "__unserialize",
        &[Param::new("data", ParamType::Array)],
        interval_serialization::unserialize,
    ),
    MethodEntry::public(
        "__set_state",
        &[Param::new("array", ParamType::Array)],
        interval_serialization::set_state,
    )
    .static_()
    .tentative_return("DateInterval"),
];

fn format(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    interval::format(context, args::object(arguments, 0), &arguments[1..])
}

pub fn add(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    interval_arithmetic::add(context, args::object(arguments, 0), &arguments[1..])
}

pub fn diff(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    interval_arithmetic::diff(context, args::object(arguments, 0), &arguments[1..])
}
