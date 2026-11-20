//! PHP calendar native signatures.

mod constants;
mod functions;
mod gregorian;
mod parameters;
mod queries;

use phera_extension_api::{
    ClassEntry, ClassHooks, MethodEntry, NativeClone, Registry, Value, Visibility,
};

use super::{
    CLASS, Factory, GREGORIAN, State, create,
    methods::{Op, invoke},
};
use crate::unsupported;
use constants::CONSTANTS;
use gregorian::GREG_METHODS;
use parameters::*;
use queries::{keywords, locales, now};

const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::copy::<State>()),
    fork: Some(phera_extension_api::fork_clone::<State>),
    ..ClassHooks::EMPTY
};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        hooks: Some(&HOOKS),
        constants: CONSTANTS,
        methods: METHODS,
        ..ClassEntry::class(CLASS)
    });
    registry.class(ClassEntry {
        parent: Some(CLASS),
        hooks: Some(&HOOKS),
        methods: GREG_METHODS,
        ..ClassEntry::class(GREGORIAN)
    });
    functions::register(registry);
}

const METHODS: &[MethodEntry] = &[
    MethodEntry::public("__construct", &[], |_, _, _| Ok(Value::Null))
        .with_visibility(Visibility::Private),
    MethodEntry::public("createInstance", CREATE, |cx, _, values| {
        create(
            cx,
            None,
            values,
            "IntlCalendar::createInstance",
            Factory::Instance,
        )
    })
    .static_()
    .returning("?IntlCalendar"),
    MethodEntry::public("fromDateTime", FROM_DATE, |cx, _, values| {
        create(
            cx,
            None,
            values,
            "IntlCalendar::fromDateTime",
            Factory::FromDate,
        )
    })
    .static_()
    .returning("?IntlCalendar"),
    MethodEntry::public("getNow", &[], |_, _, _| Ok(now()))
        .static_()
        .returning("float"),
    MethodEntry::public("getAvailableLocales", &[], |_, _, _| Ok(locales()))
        .static_()
        .returning("array"),
    MethodEntry::public("getKeywordValuesForLocale", KEYWORDS, |cx, _, values| {
        keywords(cx, values, "IntlCalendar::getKeywordValuesForLocale")
    })
    .static_()
    .returning("IntlIterator|false"),
    MethodEntry::public("fieldDifference", DIFFERENCE, |_, _, _| {
        Err(unsupported::item("IntlCalendar::fieldDifference"))
    })
    .returning("int|false"),
    MethodEntry::public("get", FIELD, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::get", Op::Get)
    })
    .returning("int|false"),
    MethodEntry::public("add", ALTER, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::add", Op::Add)
    })
    .returning("bool"),
    MethodEntry::public("roll", ROLL, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::roll", Op::Roll)
    })
    .returning("bool"),
    MethodEntry::public("set", SET, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::set", Op::Set)
    })
    .returning("true"),
    MethodEntry::public("getTime", &[], |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::getTime", Op::Time)
    })
    .returning("float|false"),
    MethodEntry::public("setTime", STAMP, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::setTime", Op::SetTime)
    })
    .returning("bool"),
    MethodEntry::public("getTimeZone", &[], |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::getTimeZone",
            Op::TimeZone,
        )
    })
    .returning("IntlTimeZone|false"),
    MethodEntry::public("setTimeZone", ZONE_ARG, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::setTimeZone",
            Op::SetTimeZone,
        )
    })
    .returning("bool"),
    MethodEntry::public("toDateTime", &[], |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::toDateTime", Op::ToDate)
    })
    .returning("DateTime|false"),
    MethodEntry::public("getFirstDayOfWeek", &[], |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::getFirstDayOfWeek",
            Op::First,
        )
    })
    .returning("int|false"),
    MethodEntry::public("setFirstDayOfWeek", DAY, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::setFirstDayOfWeek",
            Op::SetFirst,
        )
    })
    .returning("true"),
    MethodEntry::public("getMinimalDaysInFirstWeek", &[], |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::getMinimalDaysInFirstWeek",
            Op::Minimal,
        )
    })
    .returning("int|false"),
    MethodEntry::public("setMinimalDaysInFirstWeek", DAYS, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::setMinimalDaysInFirstWeek",
            Op::SetMinimal,
        )
    })
    .returning("true"),
    MethodEntry::public("isWeekend", WEEKEND, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::isWeekend", Op::Weekend)
    })
    .returning("bool"),
    MethodEntry::public("getDayOfWeekType", DAY, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::getDayOfWeekType",
            Op::DayType,
        )
    })
    .returning("int|false"),
    MethodEntry::public("getWeekendTransition", DAY, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::getWeekendTransition",
            Op::WeekendTransition,
        )
    })
    .returning("int|false"),
    MethodEntry::public("before", OTHER, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::before", Op::Before)
    })
    .returning("bool"),
    MethodEntry::public("after", OTHER, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::after", Op::After)
    })
    .returning("bool"),
    MethodEntry::public("equals", OTHER, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::equals", Op::Equals)
    })
    .returning("bool"),
    MethodEntry::public("isEquivalentTo", OTHER, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::isEquivalentTo",
            Op::Equivalent,
        )
    })
    .returning("bool"),
    MethodEntry::public("isLenient", &[], |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::isLenient", Op::Lenient)
    })
    .returning("bool"),
    MethodEntry::public("setLenient", LENIENT, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::setLenient",
            Op::SetLenient,
        )
    })
    .returning("true"),
    MethodEntry::public("getLocale", TYPE, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::getLocale", Op::Locale)
    })
    .returning("string|false"),
    MethodEntry::public("getType", &[], |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::getType", Op::Kind)
    })
    .returning("string"),
    MethodEntry::public("inDaylightTime", &[], |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::inDaylightTime", Op::Dst)
    })
    .returning("bool"),
    MethodEntry::public("clear", CLEAR, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::clear", Op::Clear)
    })
    .returning("true"),
    MethodEntry::public("isSet", FIELD, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::isSet", Op::IsSet)
    })
    .returning("bool"),
    MethodEntry::public("getMinimum", FIELD, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::getMinimum", Op::Minimum)
    })
    .returning("int|false"),
    MethodEntry::public("getMaximum", FIELD, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::getMaximum", Op::Maximum)
    })
    .returning("int|false"),
    MethodEntry::public("getActualMinimum", FIELD, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::getActualMinimum",
            Op::ActualMin,
        )
    })
    .returning("int|false"),
    MethodEntry::public("getActualMaximum", FIELD, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::getActualMaximum",
            Op::ActualMax,
        )
    })
    .returning("int|false"),
    MethodEntry::public("getGreatestMinimum", FIELD, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::getGreatestMinimum",
            Op::GreatestMin,
        )
    })
    .returning("int|false"),
    MethodEntry::public("getLeastMaximum", FIELD, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::getLeastMaximum",
            Op::LeastMax,
        )
    })
    .returning("int|false"),
    MethodEntry::public("getErrorCode", &[], |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::getErrorCode",
            Op::ErrorCode,
        )
    })
    .returning("int|false"),
    MethodEntry::public("getErrorMessage", &[], |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::getErrorMessage",
            Op::ErrorMessage,
        )
    })
    .returning("string|false"),
    MethodEntry::public("getRepeatedWallTimeOption", &[], |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::getRepeatedWallTimeOption",
            Op::Repeated,
        )
    })
    .returning("int"),
    MethodEntry::public("getSkippedWallTimeOption", &[], |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::getSkippedWallTimeOption",
            Op::Skipped,
        )
    })
    .returning("int"),
    MethodEntry::public("setRepeatedWallTimeOption", OPTION, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::setRepeatedWallTimeOption",
            Op::SetRepeated,
        )
    })
    .returning("true"),
    MethodEntry::public("setSkippedWallTimeOption", OPTION, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::setSkippedWallTimeOption",
            Op::SetSkipped,
        )
    })
    .returning("true"),
    MethodEntry::public("setDate", CIVIL, |cx, object, values| {
        invoke(cx, object, values, "IntlCalendar::setDate", Op::SetDate)
    })
    .returning("void"),
    MethodEntry::public("setDateTime", CIVIL_FULL, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlCalendar::setDateTime",
            Op::SetDateTime,
        )
    })
    .returning("void"),
];
