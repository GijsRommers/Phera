//! PHP bindings for IntlDateFormatter and the datefmt_* functions.
//!
//! ```php
//! datefmt_get_pattern($formatter) === $formatter->getPattern(); // true
//! ```

use phera_extension_api::{ClassEntry, ClassHooks, ConstantValue, FunctionEntry, MethodEntry};
use phera_extension_api::{NativeClone, Param, ParamType, Registry};

use super::{CLASS, Formatter, Operation, dispatch, init, input};

const RECEIVER: Param = Param::new("formatter", ParamType::Class("IntlDateFormatter"));
const LOCALE: Param = Param::new("locale", ParamType::String).nullable();
const TZ: Param = Param::new("timezone", ParamType::Mixed);
const CAL: Param = Param::new("calendar", ParamType::Mixed);
const PATTERN_ARG: Param = Param::new("pattern", ParamType::String);
const LENGTH_DATE: Param = Param::new("dateType", ParamType::Int)
    .with_constant_default("IntlDateFormatter::FULL", ConstantValue::Int(0));
const LENGTH_TIME: Param = Param::new("timeType", ParamType::Int)
    .with_constant_default("IntlDateFormatter::FULL", ConstantValue::Int(0));

const INIT: &[Param] = &[
    LOCALE,
    LENGTH_DATE,
    LENGTH_TIME,
    TZ.with_default(ConstantValue::Null),
    CAL.with_default(ConstantValue::Null),
    PATTERN_ARG.nullable().with_default(ConstantValue::Null),
];

const FORMAT_OBJECT: &[Param] = &[
    Param::new("datetime", ParamType::Mixed),
    Param::new("format", ParamType::Mixed).with_default(ConstantValue::Null),
    LOCALE.with_default(ConstantValue::Null),
];
const STRING: Param = Param::new("string", ParamType::String);
const OFFSET: Param = Param::new("offset", ParamType::Mixed)
    .by_ref()
    .with_default(ConstantValue::Null);
const TYPE: Param = Param::new("type", ParamType::Int)
    .with_constant_default("ULOC_ACTUAL_LOCALE", ConstantValue::Int(0));
const DATE_VALUE: Param = Param::new("datetime", ParamType::Mixed);
const LENIENT_VALUE: Param = Param::new("lenient", ParamType::Bool);
const PARSE: &[Param] = &[STRING, OFFSET];
const FN_PARSE: &[Param] = &[RECEIVER, STRING, OFFSET];
const DATETIME: &[Param] = &[DATE_VALUE];
const FN_DATETIME: &[Param] = &[RECEIVER, DATE_VALUE];
const ZONE: &[Param] = &[TZ];
const FN_ZONE: &[Param] = &[RECEIVER, TZ];
const CALENDAR: &[Param] = &[CAL];
const FN_CALENDAR: &[Param] = &[RECEIVER, CAL];
const PATTERN: &[Param] = &[PATTERN_ARG];
const FN_PATTERN: &[Param] = &[RECEIVER, PATTERN_ARG];
const LENIENT: &[Param] = &[LENIENT_VALUE];
const FN_LENIENT: &[Param] = &[RECEIVER, LENIENT_VALUE];
const LOCALE_QUERY: &[Param] = &[TYPE];
const FN_LOCALE_QUERY: &[Param] = &[RECEIVER, TYPE];
const FN_NONE: &[Param] = &[RECEIVER];

const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::copy::<Formatter>()),
    ..ClassHooks::EMPTY
};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        constants: &[
            ("FULL", ConstantValue::Int(0)),
            ("LONG", ConstantValue::Int(1)),
            ("MEDIUM", ConstantValue::Int(2)),
            ("SHORT", ConstantValue::Int(3)),
            ("NONE", ConstantValue::Int(-1)),
            ("RELATIVE_FULL", ConstantValue::Int(128)),
            ("RELATIVE_LONG", ConstantValue::Int(129)),
            ("RELATIVE_MEDIUM", ConstantValue::Int(130)),
            ("RELATIVE_SHORT", ConstantValue::Int(131)),
            ("PATTERN", ConstantValue::Int(-2)),
            ("GREGORIAN", ConstantValue::Int(1)),
            ("TRADITIONAL", ConstantValue::Int(0)),
        ],
        hooks: Some(&HOOKS),
        methods: const {
            &[
                MethodEntry::public("__construct", INIT, |c, o, a| {
                    init(c, o, a, "IntlDateFormatter::__construct")
                }),
                MethodEntry::public("create", INIT, |c, _, a| {
                    init(c, None, a, "IntlDateFormatter::create")
                })
                .static_()
                .tentative_return("?IntlDateFormatter"),
                MethodEntry::public("formatObject", FORMAT_OBJECT, |c, _, a| {
                    input::format_object(c, a, "IntlDateFormatter::formatObject")
                })
                .static_()
                .tentative_return("string|false"),
                MethodEntry::public("getDateType", &[], |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "IntlDateFormatter::getDateType",
                        Operation::DateType,
                    )
                })
                .tentative_return("int|false"),
                MethodEntry::public("getTimeType", &[], |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "IntlDateFormatter::getTimeType",
                        Operation::TimeType,
                    )
                })
                .tentative_return("int|false"),
                MethodEntry::public("getCalendar", &[], |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "IntlDateFormatter::getCalendar",
                        Operation::Calendar,
                    )
                })
                .tentative_return("int|false"),
                MethodEntry::public("setCalendar", CALENDAR, |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "IntlDateFormatter::setCalendar",
                        Operation::SetCalendar,
                    )
                })
                .tentative_return("bool"),
                MethodEntry::public("getCalendarObject", &[], |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "IntlDateFormatter::getCalendarObject",
                        Operation::CalendarObject,
                    )
                })
                .tentative_return("IntlCalendar|false|null"),
                MethodEntry::public("getTimeZone", &[], |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "IntlDateFormatter::getTimeZone",
                        Operation::TimeZone,
                    )
                })
                .tentative_return("IntlTimeZone|false"),
                MethodEntry::public("getTimeZoneId", &[], |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "IntlDateFormatter::getTimeZoneId",
                        Operation::TimeZoneId,
                    )
                })
                .tentative_return("string|false"),
                MethodEntry::public("setTimeZone", ZONE, |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "IntlDateFormatter::setTimeZone",
                        Operation::SetTimeZone,
                    )
                })
                .tentative_return("bool"),
                MethodEntry::public("getPattern", &[], |c, o, a| {
                    dispatch(c, o, a, "IntlDateFormatter::getPattern", Operation::Pattern)
                })
                .tentative_return("string|false"),
                MethodEntry::public("setPattern", PATTERN, |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "IntlDateFormatter::setPattern",
                        Operation::SetPattern,
                    )
                })
                .tentative_return("bool"),
                MethodEntry::public("getLocale", LOCALE_QUERY, |c, o, a| {
                    dispatch(c, o, a, "IntlDateFormatter::getLocale", Operation::Locale)
                })
                .tentative_return("string|false"),
                MethodEntry::public("setLenient", LENIENT, |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "IntlDateFormatter::setLenient",
                        Operation::SetLenient,
                    )
                })
                .tentative_return("void"),
                MethodEntry::public("isLenient", &[], |c, o, a| {
                    dispatch(c, o, a, "IntlDateFormatter::isLenient", Operation::Lenient)
                })
                .tentative_return("bool"),
                MethodEntry::public("format", DATETIME, |c, o, a| {
                    dispatch(c, o, a, "IntlDateFormatter::format", Operation::Format)
                })
                .tentative_return("string|false"),
                MethodEntry::public("parse", PARSE, |c, o, a| {
                    dispatch(c, o, a, "IntlDateFormatter::parse", Operation::Parse)
                })
                .tentative_return("int|float|false"),
                MethodEntry::public("parseToCalendar", PARSE, |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "IntlDateFormatter::parseToCalendar",
                        Operation::ParseToCalendar,
                    )
                })
                .returning("int|float|false"),
                MethodEntry::public("localtime", PARSE, |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "IntlDateFormatter::localtime",
                        Operation::Localtime,
                    )
                })
                .tentative_return("array|false"),
                MethodEntry::public("getErrorCode", &[], |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "IntlDateFormatter::getErrorCode",
                        Operation::ErrorCode,
                    )
                })
                .tentative_return("int"),
                MethodEntry::public("getErrorMessage", &[], |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "IntlDateFormatter::getErrorMessage",
                        Operation::ErrorMessage,
                    )
                })
                .tentative_return("string"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
    for (entry, result) in [
        (
            FunctionEntry::sync("datefmt_create", INIT, |c, a| {
                init(c, None, a, "datefmt_create")
            }),
            "?IntlDateFormatter",
        ),
        (
            FunctionEntry::sync("datefmt_format_object", FORMAT_OBJECT, |c, a| {
                input::format_object(c, a, "datefmt_format_object")
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("datefmt_get_datetype", FN_NONE, |c, a| {
                dispatch(c, None, a, "datefmt_get_datetype", Operation::DateType)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("datefmt_get_timetype", FN_NONE, |c, a| {
                dispatch(c, None, a, "datefmt_get_timetype", Operation::TimeType)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("datefmt_get_calendar", FN_NONE, |c, a| {
                dispatch(c, None, a, "datefmt_get_calendar", Operation::Calendar)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("datefmt_set_calendar", FN_CALENDAR, |c, a| {
                dispatch(c, None, a, "datefmt_set_calendar", Operation::SetCalendar)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("datefmt_get_calendar_object", FN_NONE, |c, a| {
                dispatch(
                    c,
                    None,
                    a,
                    "datefmt_get_calendar_object",
                    Operation::CalendarObject,
                )
            }),
            "IntlCalendar|false|null",
        ),
        (
            FunctionEntry::sync("datefmt_get_timezone", FN_NONE, |c, a| {
                dispatch(c, None, a, "datefmt_get_timezone", Operation::TimeZone)
            }),
            "IntlTimeZone|false",
        ),
        (
            FunctionEntry::sync("datefmt_get_timezone_id", FN_NONE, |c, a| {
                dispatch(c, None, a, "datefmt_get_timezone_id", Operation::TimeZoneId)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("datefmt_set_timezone", FN_ZONE, |c, a| {
                dispatch(c, None, a, "datefmt_set_timezone", Operation::SetTimeZone)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("datefmt_get_pattern", FN_NONE, |c, a| {
                dispatch(c, None, a, "datefmt_get_pattern", Operation::Pattern)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("datefmt_set_pattern", FN_PATTERN, |c, a| {
                dispatch(c, None, a, "datefmt_set_pattern", Operation::SetPattern)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("datefmt_get_locale", FN_LOCALE_QUERY, |c, a| {
                dispatch(c, None, a, "datefmt_get_locale", Operation::Locale)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("datefmt_set_lenient", FN_LENIENT, |c, a| {
                dispatch(c, None, a, "datefmt_set_lenient", Operation::SetLenient)
            }),
            "void",
        ),
        (
            FunctionEntry::sync("datefmt_is_lenient", FN_NONE, |c, a| {
                dispatch(c, None, a, "datefmt_is_lenient", Operation::Lenient)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("datefmt_format", FN_DATETIME, |c, a| {
                dispatch(c, None, a, "datefmt_format", Operation::Format)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("datefmt_parse", FN_PARSE, |c, a| {
                dispatch(c, None, a, "datefmt_parse", Operation::Parse)
            }),
            "int|float|false",
        ),
        (
            FunctionEntry::sync("datefmt_localtime", FN_PARSE, |c, a| {
                dispatch(c, None, a, "datefmt_localtime", Operation::Localtime)
            }),
            "array|false",
        ),
        (
            FunctionEntry::sync("datefmt_get_error_code", FN_NONE, |c, a| {
                dispatch(c, None, a, "datefmt_get_error_code", Operation::ErrorCode)
            }),
            "int",
        ),
        (
            FunctionEntry::sync("datefmt_get_error_message", FN_NONE, |c, a| {
                dispatch(
                    c,
                    None,
                    a,
                    "datefmt_get_error_message",
                    Operation::ErrorMessage,
                )
            }),
            "string",
        ),
    ] {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
}
