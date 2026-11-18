//! `DateTimeImmutable::ATOM` is inherited from DateTimeInterface.

use phera_extension_api::{ClassEntry, ConstantValue, Context, MethodEntry};
use phera_extension_api::{NativeResult, Param, ParamType, Registry, Value};

use super::instant::{IMMUTABLE, INTERFACE, MUTABLE};
use super::{
    This, datetime, datetime_factory as factory, datetime_serialization, interval_arithmetic,
    interval_registration::INTERVAL,
};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        constants: FORMAT_CONSTANTS,
        methods: INTERFACE_METHODS,
        ..ClassEntry::interface(INTERFACE)
    });
    for (class, methods) in [(MUTABLE, MUTABLE_METHODS), (IMMUTABLE, IMMUTABLE_METHODS)] {
        registry.class(ClassEntry {
            interfaces: &[INTERFACE],
            methods,
            ..ClassEntry::class(class)
        });
    }
}

pub const FORMAT_CONSTANTS: &[(&str, ConstantValue)] = &[
    ("ATOM", ConstantValue::String("Y-m-d\\TH:i:sP")),
    ("COOKIE", ConstantValue::String("l, d-M-Y H:i:s T")),
    ("ISO8601", ConstantValue::String("Y-m-d\\TH:i:sO")),
    ("ISO8601_EXPANDED", ConstantValue::String("X-m-d\\TH:i:sP")),
    ("RFC822", ConstantValue::String("D, d M y H:i:s O")),
    ("RFC850", ConstantValue::String("l, d-M-y H:i:s T")),
    ("RFC1036", ConstantValue::String("D, d M y H:i:s O")),
    ("RFC1123", ConstantValue::String("D, d M Y H:i:s O")),
    ("RFC2822", ConstantValue::String("D, d M Y H:i:s O")),
    ("RFC3339", ConstantValue::String("Y-m-d\\TH:i:sP")),
    (
        "RFC3339_EXTENDED",
        ConstantValue::String("Y-m-d\\TH:i:s.vP"),
    ),
    ("RSS", ConstantValue::String("D, d M Y H:i:s O")),
    ("W3C", ConstantValue::String("Y-m-d\\TH:i:sP")),
];
pub(super) const DATE: Param = Param::new("object", ParamType::Class(INTERFACE));
pub(super) const TARGET: Param = Param::new("targetObject", DATE.ty);
pub(super) const ABSOLUTE: Param = Param::new("absolute", ParamType::Bool).optional();
pub(super) const MUTABLE_DATE: Param = Param::new("object", ParamType::Class(MUTABLE));
pub(super) const ZONE: Param = Param::new("timezone", ParamType::Class("DateTimeZone"));
pub(super) const FORMAT: Param = Param::new("format", ParamType::String);
pub(super) const MODIFIER: Param = Param::new("modifier", ParamType::String);
pub(super) const TIMESTAMP: Param = Param::new("timestamp", ParamType::Int);
pub(super) const HOUR: Param = Param::new("hour", ParamType::Int);
pub(super) const MINUTE: Param = Param::new("minute", ParamType::Int);
pub(super) const SECOND: Param = Param::new("second", ParamType::Int).optional();
const MICROS_REQUIRED: Param = Param::new("microsecond", ParamType::Int);
pub(super) const MICROS: Param = MICROS_REQUIRED.optional();
pub(super) const YEAR: Param = Param::new("year", ParamType::Int);
pub(super) const MONTH: Param = Param::new("month", ParamType::Int);
pub(super) const DAY: Param = Param::new("day", ParamType::Int);
pub(super) const WEEK: Param = Param::new("week", ParamType::Int);
pub(super) const WEEKDAY: Param = Param::new("dayOfWeek", ParamType::Int).optional();
pub(super) const CONSTRUCT: &[Param] = &[
    Param::new("datetime", ParamType::String).optional(),
    ZONE.nullable().optional(),
];
pub(super) const FROM_FORMAT: &[Param] = &[
    FORMAT,
    Param::new("datetime", ParamType::String),
    ZONE.nullable().optional(),
];
const FROM_TIMESTAMP: &[Param] = &[Param::new("timestamp", ParamType::Number)];
const SERIAL_DATA: &[Param] = &[Param::new("data", ParamType::Array)];
const STATE_DATA: &[Param] = &[Param::new("array", ParamType::Array)];
const INTERFACE_METHODS: &[MethodEntry] = &[
    MethodEntry::required("format", &[FORMAT]),
    MethodEntry::required("getTimezone", &[]),
    MethodEntry::required("getOffset", &[]),
    MethodEntry::required("getTimestamp", &[]),
    MethodEntry::required("getMicrosecond", &[]),
    MethodEntry::required("diff", &[TARGET, ABSOLUTE]),
    MethodEntry::required("__serialize", &[]),
    MethodEntry::required("__unserialize", SERIAL_DATA),
];
const CONSTRUCTOR: MethodEntry = MethodEntry::public("__construct", CONSTRUCT, factory::construct);
const COMMON_METHODS: &[MethodEntry] = &[
    CONSTRUCTOR,
    MethodEntry::public("__serialize", &[], datetime_serialization::serialize),
    MethodEntry::public(
        "__unserialize",
        SERIAL_DATA,
        datetime_serialization::unserialize,
    ),
    MethodEntry::public("add", &[INTERVAL], interval_arithmetic::add),
    MethodEntry::public("sub", &[INTERVAL], interval_arithmetic::sub),
    MethodEntry::public("diff", &[TARGET, ABSOLUTE], interval_arithmetic::diff),
    MethodEntry::public("format", &[FORMAT], datetime::format),
    MethodEntry::public("getTimezone", &[], datetime::timezone),
    MethodEntry::public("getOffset", &[], datetime::offset),
    MethodEntry::public("getTimestamp", &[], datetime::timestamp),
    MethodEntry::public("getMicrosecond", &[], datetime::micros),
    MethodEntry::public("getLastErrors", &[], factory::last_errors).static_(),
    MethodEntry::public("modify", &[MODIFIER], datetime::modify),
    MethodEntry::public("setTimezone", &[ZONE], datetime::set_zone),
    MethodEntry::public("setTimestamp", &[TIMESTAMP], datetime::set_timestamp),
    MethodEntry::public("setMicrosecond", &[MICROS_REQUIRED], datetime::set_micros),
    MethodEntry::public(
        "setTime",
        &[HOUR, MINUTE, SECOND, MICROS],
        datetime::set_time,
    ),
    MethodEntry::public("setDate", &[YEAR, MONTH, DAY], datetime::set_date),
    MethodEntry::public("setISODate", &[YEAR, WEEK, WEEKDAY], datetime::set_iso),
];

const fn date_methods(factories: [MethodEntry; 5]) -> [MethodEntry; COMMON_METHODS.len() + 5] {
    let mut methods = [CONSTRUCTOR; COMMON_METHODS.len() + 5];
    let mut index = 0;
    while index < COMMON_METHODS.len() {
        methods[index] = COMMON_METHODS[index];
        index += 1;
    }
    let mut factory = 0;
    while factory < factories.len() {
        methods[index + factory] = factories[factory];
        factory += 1;
    }
    methods
}

const MUTABLE_METHODS: &[MethodEntry] = &date_methods([
    MethodEntry::public(
        "__set_state",
        STATE_DATA,
        datetime_serialization::set_mutable,
    )
    .static_(),
    MethodEntry::public("createFromFormat", FROM_FORMAT, mutable_format).static_(),
    MethodEntry::public("createFromTimestamp", FROM_TIMESTAMP, mutable_timestamp).static_(),
    MethodEntry::public("createFromInterface", &[DATE], mutable_object).static_(),
    MethodEntry::public(
        "createFromImmutable",
        &[Param::new("object", ParamType::Class(IMMUTABLE))],
        mutable_object,
    )
    .static_(),
]);

const IMMUTABLE_METHODS: &[MethodEntry] = &date_methods([
    MethodEntry::public(
        "__set_state",
        STATE_DATA,
        datetime_serialization::set_immutable,
    )
    .static_(),
    MethodEntry::public("createFromFormat", FROM_FORMAT, immutable_format).static_(),
    MethodEntry::public("createFromTimestamp", FROM_TIMESTAMP, immutable_timestamp).static_(),
    MethodEntry::public("createFromInterface", &[DATE], immutable_object).static_(),
    MethodEntry::public("createFromMutable", &[MUTABLE_DATE], immutable_object).static_(),
]);

fn mutable_format(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    factory::create(
        context,
        arguments,
        MUTABLE,
        "DateTime::createFromFormat",
        true,
    )
}

fn immutable_format(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    factory::create(
        context,
        arguments,
        IMMUTABLE,
        "DateTimeImmutable::createFromFormat",
        true,
    )
}

fn mutable_timestamp(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    factory::from_timestamp(context, arguments, MUTABLE)
}

fn immutable_timestamp(
    context: &mut dyn Context,
    _: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    factory::from_timestamp(context, arguments, IMMUTABLE)
}

fn mutable_object(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    factory::from_object(context, arguments, MUTABLE)
}

fn immutable_object(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    factory::from_object(context, arguments, IMMUTABLE)
}
