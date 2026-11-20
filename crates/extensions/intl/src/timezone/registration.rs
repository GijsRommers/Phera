//! Date/time zone native registration. Timestamp and output offsets use milliseconds.

mod functions;

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue, MethodEntry, NativeClone, Param, ParamType, Registry,
    Value, Visibility,
};

use super::{CLASS, Factory, Op, State, dispatch, factory, ids};

const ZONE: Param = Param::new("timezone", ParamType::Class(CLASS));
const ID: Param = Param::new("timezoneId", ParamType::String);
const REGION: Param = Param::new("region", ParamType::String)
    .nullable()
    .with_default(ConstantValue::Null);
const ID_ONLY: &[Param] = &[ID];
const INSTANCE: &[Param] = &[ZONE];
const DATE_ZONE: &[Param] = &[Param::new("timezone", ParamType::Class("DateTimeZone"))];
const CANONICAL: &[Param] = &[
    ID,
    Param::new("isSystemId", ParamType::Mixed)
        .by_ref()
        .with_default(ConstantValue::Null),
];
const EQUIVALENT: &[Param] = &[ID, Param::new("offset", ParamType::Int)];
const WINDOWS: &[Param] = &[ID, REGION];
const ENUMERATE: &[Param] = &[Param::new("countryOrRawOffset", ParamType::IntOrString)
    .nullable()
    .with_default(ConstantValue::Null)];
const ENUMERATE_TYPED: &[Param] = &[
    Param::new("type", ParamType::Int),
    REGION,
    Param::new("rawOffset", ParamType::Int)
        .nullable()
        .with_default(ConstantValue::Null),
];
const DST: Param = Param::new("dst", ParamType::Bool).with_default(ConstantValue::Bool(false));
const STYLE: Param = Param::new("style", ParamType::Int)
    .with_constant_default("IntlTimeZone::DISPLAY_LONG", ConstantValue::Int(1));
const LOCALE: Param = Param::new("locale", ParamType::String)
    .nullable()
    .with_default(ConstantValue::Null);
const DISPLAY: &[Param] = &[DST, STYLE, LOCALE];
const FN_DISPLAY: &[Param] = &[ZONE, DST, STYLE, LOCALE];
const STAMP: Param = Param::new("timestamp", ParamType::Float);
const LOCAL: Param = Param::new("local", ParamType::Bool);
const RAW_OUT: Param = Param::new("rawOffset", ParamType::Mixed).by_ref();
const DST_OUT: Param = Param::new("dstOffset", ParamType::Mixed).by_ref();
const OFFSET: &[Param] = &[STAMP, LOCAL, RAW_OUT, DST_OUT];
const FN_OFFSET: &[Param] = &[ZONE, STAMP, LOCAL, RAW_OUT, DST_OUT];
const OTHER: Param = Param::new("other", ParamType::Class(CLASS));
const RULES: &[Param] = &[OTHER];
const FN_RULES: &[Param] = &[ZONE, OTHER];
const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::copy::<State>()),
    fork: Some(phera_extension_api::fork_clone::<State>),
    ..ClassHooks::EMPTY
};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        hooks: Some(&HOOKS),
        constants: &[
            ("DISPLAY_SHORT", ConstantValue::Int(0)),
            ("DISPLAY_LONG", ConstantValue::Int(1)),
            ("DISPLAY_SHORT_GENERIC", ConstantValue::Int(2)),
            ("DISPLAY_LONG_GENERIC", ConstantValue::Int(3)),
            ("DISPLAY_SHORT_GMT", ConstantValue::Int(4)),
            ("DISPLAY_LONG_GMT", ConstantValue::Int(5)),
            ("DISPLAY_SHORT_COMMONLY_USED", ConstantValue::Int(6)),
            ("DISPLAY_GENERIC_LOCATION", ConstantValue::Int(7)),
            ("TYPE_ANY", ConstantValue::Int(0)),
            ("TYPE_CANONICAL", ConstantValue::Int(1)),
            ("TYPE_CANONICAL_LOCATION", ConstantValue::Int(2)),
        ],
        methods: const {
            &[
                MethodEntry::public("__construct", &[], |_, _, _| Ok(Value::Null))
                    .with_visibility(Visibility::Private),
                MethodEntry::public("createDefault", &[], |cx, _, values| {
                    factory(cx, values, Factory::Default)
                })
                .static_()
                .returning("IntlTimeZone"),
                MethodEntry::public("createTimeZone", ID_ONLY, |cx, _, values| {
                    factory(cx, values, Factory::Named)
                })
                .static_()
                .returning("?IntlTimeZone"),
                MethodEntry::public("getGMT", &[], |cx, _, values| {
                    factory(cx, values, Factory::Gmt)
                })
                .static_()
                .returning("IntlTimeZone"),
                MethodEntry::public("getUnknown", &[], |cx, _, values| {
                    factory(cx, values, Factory::Unknown)
                })
                .static_()
                .returning("IntlTimeZone"),
                MethodEntry::public("fromDateTimeZone", DATE_ZONE, |cx, _, values| {
                    factory(cx, values, Factory::FromDate)
                })
                .static_()
                .returning("?IntlTimeZone"),
                MethodEntry::public("countEquivalentIDs", ID_ONLY, |cx, _, values| {
                    ids::invoke(
                        cx,
                        values,
                        "IntlTimeZone::countEquivalentIDs",
                        ids::Op::Count,
                    )
                })
                .static_()
                .returning("int|false"),
                MethodEntry::public("getEquivalentID", EQUIVALENT, |cx, _, values| {
                    ids::invoke(
                        cx,
                        values,
                        "IntlTimeZone::getEquivalentID",
                        ids::Op::Equivalent,
                    )
                })
                .static_()
                .returning("string|false"),
                MethodEntry::public("getCanonicalID", CANONICAL, |cx, _, values| {
                    ids::invoke(
                        cx,
                        values,
                        "IntlTimeZone::getCanonicalID",
                        ids::Op::Canonical,
                    )
                })
                .static_()
                .returning("string|false"),
                MethodEntry::public("getIanaID", ID_ONLY, |cx, _, values| {
                    ids::invoke(cx, values, "IntlTimeZone::getIanaID", ids::Op::Iana)
                })
                .static_()
                .returning("string|false"),
                MethodEntry::public("getRegion", ID_ONLY, |cx, _, values| {
                    ids::invoke(cx, values, "IntlTimeZone::getRegion", ids::Op::Region)
                })
                .static_()
                .returning("string|false"),
                MethodEntry::public("getTZDataVersion", &[], |cx, _, values| {
                    ids::invoke(
                        cx,
                        values,
                        "IntlTimeZone::getTZDataVersion",
                        ids::Op::Version,
                    )
                })
                .static_()
                .returning("string|false"),
                MethodEntry::public("getWindowsID", ID_ONLY, |cx, _, values| {
                    ids::invoke(cx, values, "IntlTimeZone::getWindowsID", ids::Op::Windows)
                })
                .static_()
                .returning("string|false"),
                MethodEntry::public("getIDForWindowsID", WINDOWS, |cx, _, values| {
                    ids::invoke(
                        cx,
                        values,
                        "IntlTimeZone::getIDForWindowsID",
                        ids::Op::FromWindows,
                    )
                })
                .static_()
                .returning("string|false"),
                MethodEntry::public("getID", &[], |cx, object, values| {
                    dispatch(cx, object, values, "IntlTimeZone::getID", Op::Id)
                })
                .returning("string|false"),
                MethodEntry::public("getRawOffset", &[], |cx, object, values| {
                    dispatch(cx, object, values, "IntlTimeZone::getRawOffset", Op::Raw)
                })
                .returning("int"),
                MethodEntry::public("getDSTSavings", &[], |cx, object, values| {
                    dispatch(
                        cx,
                        object,
                        values,
                        "IntlTimeZone::getDSTSavings",
                        Op::Savings,
                    )
                })
                .returning("int"),
                MethodEntry::public("useDaylightTime", &[], |cx, object, values| {
                    dispatch(
                        cx,
                        object,
                        values,
                        "IntlTimeZone::useDaylightTime",
                        Op::Daylight,
                    )
                })
                .returning("bool"),
                MethodEntry::public("getOffset", OFFSET, |cx, object, values| {
                    dispatch(cx, object, values, "IntlTimeZone::getOffset", Op::Offset)
                })
                .returning("bool"),
                MethodEntry::public("getDisplayName", DISPLAY, |cx, object, values| {
                    dispatch(
                        cx,
                        object,
                        values,
                        "IntlTimeZone::getDisplayName",
                        Op::Display,
                    )
                })
                .returning("string|false"),
                MethodEntry::public("hasSameRules", RULES, |cx, object, values| {
                    dispatch(
                        cx,
                        object,
                        values,
                        "IntlTimeZone::hasSameRules",
                        Op::SameRules,
                    )
                })
                .returning("bool"),
                MethodEntry::public("toDateTimeZone", &[], |cx, object, values| {
                    dispatch(
                        cx,
                        object,
                        values,
                        "IntlTimeZone::toDateTimeZone",
                        Op::ToDate,
                    )
                })
                .returning("DateTimeZone|false"),
                MethodEntry::public("getErrorCode", &[], |cx, object, values| {
                    dispatch(
                        cx,
                        object,
                        values,
                        "IntlTimeZone::getErrorCode",
                        Op::ErrorCode,
                    )
                })
                .returning("int|false"),
                MethodEntry::public("getErrorMessage", &[], |cx, object, values| {
                    dispatch(
                        cx,
                        object,
                        values,
                        "IntlTimeZone::getErrorMessage",
                        Op::ErrorMessage,
                    )
                })
                .returning("string|false"),
                MethodEntry::public("createEnumeration", ENUMERATE, |cx, _, values| {
                    ids::enumerate(cx, values, "IntlTimeZone::createEnumeration", false)
                })
                .static_()
                .returning("IntlIterator|false"),
                MethodEntry::public(
                    "createTimeZoneIDEnumeration",
                    ENUMERATE_TYPED,
                    |cx, _, values| {
                        ids::enumerate(
                            cx,
                            values,
                            "IntlTimeZone::createTimeZoneIDEnumeration",
                            true,
                        )
                    },
                )
                .static_()
                .returning("IntlIterator|false"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
    functions::register(registry);
}
