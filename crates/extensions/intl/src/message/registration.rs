//! PHP message formatting methods and their procedural names.

use phera_extension_api::{
    ClassEntry, ClassHooks, FunctionEntry, MethodEntry, NativeClone, Param, ParamType, Registry,
};

use super::{CLASS, Op, State, create, dispatch, standalone};

const LOCALE: Param = Param::new("locale", ParamType::String);
const PATTERN: Param = Param::new("pattern", ParamType::String);
const VALUES: Param = Param::new("values", ParamType::Array);
const STRING: Param = Param::new("string", ParamType::String);
const MESSAGE: Param = Param::new("message", ParamType::String);
const OBJECT: Param = Param::new("formatter", ParamType::Class(CLASS));
const CREATE: &[Param] = &[LOCALE, PATTERN];
const FORMAT: &[Param] = &[VALUES];
const PARSE: &[Param] = &[STRING];
const FN_FORMAT: &[Param] = &[OBJECT, VALUES];
const FN_PARSE: &[Param] = &[OBJECT, STRING];
const PATTERN_ARG: &[Param] = &[PATTERN];
const FN_PATTERN: &[Param] = &[OBJECT, PATTERN];
const INSTANCE: &[Param] = &[OBJECT];
const FORMAT_MESSAGE: &[Param] = &[LOCALE, PATTERN, VALUES];
const PARSE_MESSAGE: &[Param] = &[LOCALE, PATTERN, MESSAGE];
const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::copy::<State>()),
    fork: Some(phera_extension_api::fork_clone::<State>),
    ..ClassHooks::EMPTY
};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        hooks: Some(&HOOKS),
        methods: const {
            &[
                MethodEntry::public("__construct", CREATE, |cx, this, values| {
                    create(cx, this, values, "MessageFormatter::__construct")
                }),
                MethodEntry::public("create", CREATE, |cx, _, values| {
                    create(cx, None, values, "MessageFormatter::create")
                })
                .static_()
                .returning("?MessageFormatter"),
                MethodEntry::public("format", FORMAT, |cx, this, values| {
                    dispatch(cx, this, values, "MessageFormatter::format", Op::Format)
                })
                .returning("string|false"),
                MethodEntry::public("parse", PARSE, |cx, this, values| {
                    dispatch(cx, this, values, "MessageFormatter::parse", Op::Parse)
                })
                .returning("array|false"),
                MethodEntry::public("setPattern", PATTERN_ARG, |cx, this, values| {
                    dispatch(
                        cx,
                        this,
                        values,
                        "MessageFormatter::setPattern",
                        Op::SetPattern,
                    )
                })
                .returning("bool"),
                MethodEntry::public("getPattern", &[], |cx, this, values| {
                    dispatch(
                        cx,
                        this,
                        values,
                        "MessageFormatter::getPattern",
                        Op::GetPattern,
                    )
                })
                .returning("string|false"),
                MethodEntry::public("getLocale", &[], |cx, this, values| {
                    dispatch(cx, this, values, "MessageFormatter::getLocale", Op::Locale)
                })
                .returning("string"),
                MethodEntry::public("getErrorCode", &[], |cx, this, values| {
                    dispatch(
                        cx,
                        this,
                        values,
                        "MessageFormatter::getErrorCode",
                        Op::ErrorCode,
                    )
                })
                .returning("int"),
                MethodEntry::public("getErrorMessage", &[], |cx, this, values| {
                    dispatch(
                        cx,
                        this,
                        values,
                        "MessageFormatter::getErrorMessage",
                        Op::ErrorMessage,
                    )
                })
                .returning("string"),
                MethodEntry::public("formatMessage", FORMAT_MESSAGE, |cx, _, values| {
                    standalone(cx, values, "MessageFormatter::formatMessage", Op::Format)
                })
                .static_()
                .returning("string|false"),
                MethodEntry::public("parseMessage", PARSE_MESSAGE, |cx, _, values| {
                    standalone(cx, values, "MessageFormatter::parseMessage", Op::Parse)
                })
                .static_()
                .returning("array|false"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
    for (entry, result) in [
        (
            FunctionEntry::sync("msgfmt_create", CREATE, |cx, values| {
                create(cx, None, values, "msgfmt_create")
            }),
            "?MessageFormatter",
        ),
        (
            FunctionEntry::sync("msgfmt_format", FN_FORMAT, |cx, values| {
                dispatch(cx, None, values, "msgfmt_format", Op::Format)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("msgfmt_parse", FN_PARSE, |cx, values| {
                dispatch(cx, None, values, "msgfmt_parse", Op::Parse)
            }),
            "array|false",
        ),
        (
            FunctionEntry::sync("msgfmt_set_pattern", FN_PATTERN, |cx, values| {
                dispatch(cx, None, values, "msgfmt_set_pattern", Op::SetPattern)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("msgfmt_get_pattern", INSTANCE, |cx, values| {
                dispatch(cx, None, values, "msgfmt_get_pattern", Op::GetPattern)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("msgfmt_get_locale", INSTANCE, |cx, values| {
                dispatch(cx, None, values, "msgfmt_get_locale", Op::Locale)
            }),
            "string",
        ),
        (
            FunctionEntry::sync("msgfmt_get_error_code", INSTANCE, |cx, values| {
                dispatch(cx, None, values, "msgfmt_get_error_code", Op::ErrorCode)
            }),
            "int",
        ),
        (
            FunctionEntry::sync("msgfmt_get_error_message", INSTANCE, |cx, values| {
                dispatch(
                    cx,
                    None,
                    values,
                    "msgfmt_get_error_message",
                    Op::ErrorMessage,
                )
            }),
            "string",
        ),
        (
            FunctionEntry::sync("msgfmt_format_message", FORMAT_MESSAGE, |cx, values| {
                standalone(cx, values, "msgfmt_format_message", Op::Format)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("msgfmt_parse_message", PARSE_MESSAGE, |cx, values| {
                standalone(cx, values, "msgfmt_parse_message", Op::Parse)
            }),
            "array|false",
        ),
    ] {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
}
