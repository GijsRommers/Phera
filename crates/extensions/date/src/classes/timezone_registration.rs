//! Signatures preserve named arguments such as `getTransitions(timestampEnd: 0)`.

use phera_extension_api::{ClassEntry, ConstantValue, Context, FunctionEntry, MethodEntry};
use phera_extension_api::{NativeResult, Param, ParamType, Registry, Value, args};

use super::{This, abbreviations, catalog, timezone};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        constants: &[
            ("AFRICA", ConstantValue::Int(1)),
            ("AMERICA", ConstantValue::Int(2)),
            ("ANTARCTICA", ConstantValue::Int(4)),
            ("ARCTIC", ConstantValue::Int(8)),
            ("ASIA", ConstantValue::Int(16)),
            ("ATLANTIC", ConstantValue::Int(32)),
            ("AUSTRALIA", ConstantValue::Int(64)),
            ("EUROPE", ConstantValue::Int(128)),
            ("INDIAN", ConstantValue::Int(256)),
            ("PACIFIC", ConstantValue::Int(512)),
            ("UTC", ConstantValue::Int(1024)),
            ("ALL", ConstantValue::Int(catalog::ALL)),
            ("ALL_WITH_BC", ConstantValue::Int(catalog::ALL_WITH_BC)),
            ("PER_COUNTRY", ConstantValue::Int(catalog::PER_COUNTRY)),
        ],
        methods: METHODS,
        hooks: Some(&super::object_hooks::ZONE),
        ..ClassEntry::class("DateTimeZone")
    });
    for entry in [
        FunctionEntry::sync("timezone_open", ZONE_INPUT, timezone::open),
        FunctionEntry::sync("timezone_name_get", &[ZONE_OBJECT], name),
        FunctionEntry::sync("timezone_offset_get", &[ZONE_OBJECT, DATE_OBJECT], offset),
        FunctionEntry::sync("timezone_location_get", &[ZONE_OBJECT], location),
        FunctionEntry::sync(
            "timezone_transitions_get",
            &[ZONE_OBJECT, BEGIN, END],
            transitions,
        ),
        FunctionEntry::sync("timezone_identifiers_list", IDENTIFIERS, identifiers),
        FunctionEntry::sync("timezone_abbreviations_list", &[], list_abbreviations),
        FunctionEntry::sync("timezone_name_from_abbr", REVERSE, reverse),
    ] {
        registry.function(entry);
    }
}

const ZONE_INPUT: &[Param] = &[Param::new("timezone", ParamType::String)];
const ZONE_OBJECT: Param = Param::new("object", ParamType::Class("DateTimeZone"));
const DATE_OBJECT: Param = Param::new("datetime", ParamType::Class("DateTimeInterface"));
const BEGIN: Param =
    Param::new("timestampBegin", ParamType::Int).with_default(ConstantValue::Int(i64::MIN));
const END: Param = Param::new("timestampEnd", ParamType::Int).optional();
const IDENTIFIERS: &[Param] = &[
    Param::new("timezoneGroup", ParamType::Int).with_default(ConstantValue::Int(catalog::ALL)),
    Param::new("countryCode", ParamType::String)
        .nullable()
        .optional(),
];
const REVERSE: &[Param] = &[
    Param::new("abbr", ParamType::String),
    Param::new("utcOffset", ParamType::Int).with_default(ConstantValue::Int(-1)),
    Param::new("isDST", ParamType::Int).optional(),
];
const METHODS: &[MethodEntry] = &[
    MethodEntry::public("__construct", ZONE_INPUT, timezone::construct),
    MethodEntry::public("getName", &[], timezone::name),
    MethodEntry::public("getOffset", &[DATE_OBJECT], timezone::offset),
    MethodEntry::public("getLocation", &[], timezone::location),
    MethodEntry::public("getTransitions", &[BEGIN, END], timezone::transitions),
    MethodEntry::public("listIdentifiers", IDENTIFIERS, method_identifiers).static_(),
    MethodEntry::public("listAbbreviations", &[], method_abbreviations).static_(),
    MethodEntry::public("__serialize", &[], timezone::serialize),
    MethodEntry::public(
        "__unserialize",
        &[Param::new("data", ParamType::Array)],
        timezone::unserialize,
    ),
    MethodEntry::public(
        "__set_state",
        &[Param::new("array", ParamType::Array)],
        timezone::set_state,
    )
    .static_()
    .tentative_return("DateTimeZone"),
];

fn name(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    timezone::name(context, args::object(arguments, 0), &arguments[1..])
}

fn offset(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    timezone::offset(context, args::object(arguments, 0), &arguments[1..])
}

fn location(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    timezone::location(context, args::object(arguments, 0), &arguments[1..])
}

fn transitions(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    timezone::transitions(context, args::object(arguments, 0), &arguments[1..])
}

fn identifiers(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    catalog::identifiers(arguments, "timezone_identifiers_list")
}

fn method_identifiers(_: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    catalog::identifiers(arguments, "DateTimeZone::listIdentifiers")
}

fn list_abbreviations(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    abbreviations::abbreviations()
}

fn method_abbreviations(_: &mut dyn Context, _: This<'_>, _: &[Value]) -> NativeResult {
    abbreviations::abbreviations()
}

fn reverse(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    abbreviations::reverse(arguments)
}
