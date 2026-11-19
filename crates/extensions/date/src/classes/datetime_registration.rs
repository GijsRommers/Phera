//! `DateTimeImmutable::ATOM` is inherited from DateTimeInterface.

use phera_extension_api::{ClassEntry, ConstantValue, Context, MethodEntry};
use phera_extension_api::{NativeResult, Param, ParamType, Registry, Value};
use phera_source::deprecations::{ItemKind, kept_constant_notice};
use std::sync::OnceLock;

use super::instant::{IMMUTABLE, INTERFACE, MUTABLE};
use super::{
    This, datetime, datetime_factory as factory, datetime_serialization, interval_arithmetic,
    interval_registration::INTERVAL,
};

pub fn register(registry: &mut Registry) {
    static DEPRECATED: OnceLock<Option<(&'static str, &'static str)>> = OnceLock::new();
    registry.class(ClassEntry {
        constants: FORMAT_CONSTANTS,
        deprecated_constants: DEPRECATED
            .get_or_init(|| {
                kept_constant_notice(ItemKind::ClassConstant, "DateTimeInterface::RFC7231")
                    .map(|notice| ("RFC7231", notice))
            })
            .as_slice(),
        methods: INTERFACE_METHODS,
        ..ClassEntry::interface(INTERFACE)
    });
    for (class, methods) in [(MUTABLE, MUTABLE_METHODS), (IMMUTABLE, IMMUTABLE_METHODS)] {
        registry.class(ClassEntry {
            interfaces: &[INTERFACE],
            hooks: Some(&super::object_hooks::DATETIME),
            methods,
            ..ClassEntry::class(class)
        });
    }
}

pub(crate) const RFC7231_FORMAT: &str = "D, d M Y H:i:s \\G\\M\\T";

pub const FORMAT_CONSTANTS: &[(&str, ConstantValue)] = &[
    ("ATOM", ConstantValue::String("Y-m-d\\TH:i:sP")),
    ("COOKIE", ConstantValue::String("l, d-M-Y H:i:s T")),
    ("ISO8601", ConstantValue::String("Y-m-d\\TH:i:sO")),
    ("ISO8601_EXPANDED", ConstantValue::String("X-m-d\\TH:i:sP")),
    ("RFC822", ConstantValue::String("D, d M y H:i:s O")),
    ("RFC850", ConstantValue::String("l, d-M-y H:i:s T")),
    ("RFC1036", ConstantValue::String("D, d M y H:i:s O")),
    ("RFC1123", ConstantValue::String("D, d M Y H:i:s O")),
    ("RFC7231", ConstantValue::String(RFC7231_FORMAT)),
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
pub(super) const ABSOLUTE: Param =
    Param::new("absolute", ParamType::Bool).with_default(ConstantValue::Bool(false));
pub(super) const MUTABLE_DATE: Param = Param::new("object", ParamType::Class(MUTABLE));
pub(super) const ZONE: Param = Param::new("timezone", ParamType::Class("DateTimeZone"));
pub(super) const FORMAT: Param = Param::new("format", ParamType::String);
pub(super) const MODIFIER: Param = Param::new("modifier", ParamType::String);
pub(super) const TIMESTAMP: Param = Param::new("timestamp", ParamType::Int);
pub(super) const HOUR: Param = Param::new("hour", ParamType::Int);
pub(super) const MINUTE: Param = Param::new("minute", ParamType::Int);
pub(super) const SECOND: Param =
    Param::new("second", ParamType::Int).with_default(ConstantValue::Int(0));
const MICROS_REQUIRED: Param = Param::new("microsecond", ParamType::Int);
pub(super) const MICROS: Param = MICROS_REQUIRED.with_default(ConstantValue::Int(0));
pub(super) const YEAR: Param = Param::new("year", ParamType::Int);
pub(super) const MONTH: Param = Param::new("month", ParamType::Int);
pub(super) const DAY: Param = Param::new("day", ParamType::Int);
pub(super) const WEEK: Param = Param::new("week", ParamType::Int);
pub(super) const WEEKDAY: Param =
    Param::new("dayOfWeek", ParamType::Int).with_default(ConstantValue::Int(1));
pub(super) const CONSTRUCT: &[Param] = &[
    Param::new("datetime", ParamType::String).with_default(ConstantValue::String("now")),
    ZONE.nullable().with_default(ConstantValue::Null),
];
pub(super) const FROM_FORMAT: &[Param] = &[
    FORMAT,
    Param::new("datetime", ParamType::String),
    ZONE.nullable().with_default(ConstantValue::Null),
];
const FROM_TIMESTAMP: &[Param] = &[Param::new("timestamp", ParamType::Number)];
const SERIAL_DATA: &[Param] = &[Param::new("data", ParamType::Array)];
const STATE_DATA: &[Param] = &[Param::new("array", ParamType::Array)];
const INTERFACE_METHODS: &[MethodEntry] = &[
    MethodEntry::required("format", &[FORMAT]).tentative_return("string"),
    MethodEntry::required("getTimezone", &[]).tentative_return("DateTimeZone|false"),
    MethodEntry::required("getOffset", &[]).tentative_return("int"),
    MethodEntry::required("getTimestamp", &[]).tentative_return("int"),
    MethodEntry::required("getMicrosecond", &[]).returning("int"),
    MethodEntry::required("diff", &[TARGET, ABSOLUTE]).tentative_return("DateInterval"),
    MethodEntry::required("__serialize", &[]).returning("array"),
    MethodEntry::required("__unserialize", SERIAL_DATA).returning("void"),
];
const CONSTRUCTOR: MethodEntry = MethodEntry::public("__construct", CONSTRUCT, factory::construct);
const SERIALIZE: MethodEntry =
    MethodEntry::public("__serialize", &[], datetime_serialization::serialize).returning("array");
const UNSERIALIZE: MethodEntry = MethodEntry::public(
    "__unserialize",
    SERIAL_DATA,
    datetime_serialization::unserialize,
)
.returning("void");
const ADD: MethodEntry = MethodEntry::public("add", &[INTERVAL], interval_arithmetic::add);
const SUB: MethodEntry = MethodEntry::public("sub", &[INTERVAL], interval_arithmetic::sub);
const DIFF: MethodEntry =
    MethodEntry::public("diff", &[TARGET, ABSOLUTE], interval_arithmetic::diff)
        .tentative_return("DateInterval");
const FORMAT_DATE: MethodEntry =
    MethodEntry::public("format", &[FORMAT], datetime::format).tentative_return("string");
const TIMEZONE: MethodEntry = MethodEntry::public("getTimezone", &[], datetime::timezone)
    .tentative_return("DateTimeZone|false");
const OFFSET: MethodEntry =
    MethodEntry::public("getOffset", &[], datetime::offset).tentative_return("int");
const GET_TIMESTAMP: MethodEntry =
    MethodEntry::public("getTimestamp", &[], datetime::timestamp).tentative_return("int");
const GET_MICROS: MethodEntry =
    MethodEntry::public("getMicrosecond", &[], datetime::micros).returning("int");
const LAST_ERRORS: MethodEntry = MethodEntry::public("getLastErrors", &[], factory::last_errors)
    .static_()
    .tentative_return("array|false");
const MODIFY: MethodEntry = MethodEntry::public("modify", &[MODIFIER], datetime::modify);
const SET_ZONE: MethodEntry = MethodEntry::public("setTimezone", &[ZONE], datetime::set_zone);
const SET_TIMESTAMP: MethodEntry =
    MethodEntry::public("setTimestamp", &[TIMESTAMP], datetime::set_timestamp);
const SET_MICROS: MethodEntry =
    MethodEntry::public("setMicrosecond", &[MICROS_REQUIRED], datetime::set_micros);
const SET_TIME: MethodEntry = MethodEntry::public(
    "setTime",
    &[HOUR, MINUTE, SECOND, MICROS],
    datetime::set_time,
);
const SET_DATE: MethodEntry =
    MethodEntry::public("setDate", &[YEAR, MONTH, DAY], datetime::set_date);
const SET_ISO: MethodEntry =
    MethodEntry::public("setISODate", &[YEAR, WEEK, WEEKDAY], datetime::set_iso);

const MUTABLE_METHODS: &[MethodEntry] = &[
    CONSTRUCTOR,
    SERIALIZE,
    UNSERIALIZE,
    MethodEntry::public(
        "__set_state",
        STATE_DATA,
        datetime_serialization::set_mutable,
    )
    .static_()
    .tentative_return(MUTABLE),
    MethodEntry::public(
        "createFromImmutable",
        &[Param::new("object", ParamType::Class(IMMUTABLE))],
        mutable_object,
    )
    .static_()
    .tentative_return("static"),
    MethodEntry::public("createFromInterface", &[DATE], mutable_object)
        .static_()
        .returning(MUTABLE),
    MethodEntry::public("createFromFormat", FROM_FORMAT, mutable_format)
        .static_()
        .tentative_return("DateTime|false"),
    MethodEntry::public("createFromTimestamp", FROM_TIMESTAMP, mutable_timestamp)
        .static_()
        .tentative_return("static"),
    LAST_ERRORS,
    FORMAT_DATE,
    MODIFY.tentative_return(MUTABLE),
    ADD.tentative_return(MUTABLE),
    SUB.tentative_return(MUTABLE),
    TIMEZONE,
    SET_ZONE.tentative_return(MUTABLE),
    OFFSET,
    GET_MICROS,
    SET_TIME.tentative_return(MUTABLE),
    SET_DATE.tentative_return(MUTABLE),
    SET_ISO.tentative_return(MUTABLE),
    SET_TIMESTAMP.tentative_return(MUTABLE),
    SET_MICROS.returning("static"),
    GET_TIMESTAMP,
    DIFF,
];

const IMMUTABLE_METHODS: &[MethodEntry] = &[
    CONSTRUCTOR,
    SERIALIZE,
    UNSERIALIZE,
    MethodEntry::public(
        "__set_state",
        STATE_DATA,
        datetime_serialization::set_immutable,
    )
    .static_()
    .tentative_return(IMMUTABLE),
    MethodEntry::public("createFromFormat", FROM_FORMAT, immutable_format)
        .static_()
        .tentative_return("DateTimeImmutable|false"),
    MethodEntry::public("createFromTimestamp", FROM_TIMESTAMP, immutable_timestamp)
        .static_()
        .tentative_return("static"),
    LAST_ERRORS,
    FORMAT_DATE,
    TIMEZONE,
    OFFSET,
    GET_TIMESTAMP,
    GET_MICROS,
    DIFF,
    MODIFY.tentative_return(IMMUTABLE),
    ADD.tentative_return(IMMUTABLE),
    SUB.tentative_return(IMMUTABLE),
    SET_ZONE.tentative_return(IMMUTABLE),
    SET_TIME.tentative_return(IMMUTABLE),
    SET_DATE.tentative_return(IMMUTABLE),
    SET_ISO.tentative_return(IMMUTABLE),
    SET_TIMESTAMP.tentative_return(IMMUTABLE),
    SET_MICROS.returning("static"),
    MethodEntry::public("createFromMutable", &[MUTABLE_DATE], immutable_object)
        .static_()
        .tentative_return("static"),
    MethodEntry::public("createFromInterface", &[DATE], immutable_object)
        .static_()
        .returning(IMMUTABLE),
];

fn mutable_format(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    let class = context.called_class().unwrap_or(MUTABLE).to_owned();
    factory::create(
        context,
        arguments,
        &class,
        "DateTime::createFromFormat",
        true,
    )
}

fn immutable_format(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    let class = context.called_class().unwrap_or(IMMUTABLE).to_owned();
    factory::create(
        context,
        arguments,
        &class,
        "DateTimeImmutable::createFromFormat",
        true,
    )
}

fn mutable_timestamp(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    let class = context.called_class().unwrap_or(MUTABLE).to_owned();
    factory::from_timestamp(context, arguments, &class, MUTABLE)
}

fn immutable_timestamp(
    context: &mut dyn Context,
    _: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    let class = context.called_class().unwrap_or(IMMUTABLE).to_owned();
    factory::from_timestamp(context, arguments, &class, IMMUTABLE)
}

fn mutable_object(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    let class = context.called_class().unwrap_or(MUTABLE).to_owned();
    factory::from_object(context, arguments, &class)
}

fn immutable_object(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    let class = context.called_class().unwrap_or(IMMUTABLE).to_owned();
    factory::from_object(context, arguments, &class)
}
