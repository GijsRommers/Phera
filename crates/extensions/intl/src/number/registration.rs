//! NumberFormatter and numfmt_* expose the same native formatter.
//!
//! ```php
//! $f = numfmt_create("en_US", NumberFormatter::DECIMAL);
//! numfmt_format($f, 1234) === $f->format(1234); // true
//! ```

use phera_extension_api::{ClassEntry, ClassHooks, ConstantValue, FunctionEntry, MethodEntry};
use phera_extension_api::{NativeClone, Param, ParamType, Registry};

use super::constants::CONSTANTS;
use super::{Formatter, Operation, dispatch, init};

const CLASS: Param = Param::new("formatter", ParamType::Class("NumberFormatter"));
const STRING: Param = Param::new("string", ParamType::String);
const VALUE: Param = Param::new("value", ParamType::String);
const ATTRIBUTE: Param = Param::new("attribute", ParamType::Int);
const SYMBOL_ARG: Param = Param::new("symbol", ParamType::Int);
const TYPE_DEFAULT: Param = Param::new("type", ParamType::Int)
    .with_constant_default("NumberFormatter::TYPE_DEFAULT", ConstantValue::Int(0));
const TYPE_DOUBLE: Param = Param::new("type", ParamType::Int)
    .with_constant_default("NumberFormatter::TYPE_DOUBLE", ConstantValue::Int(3));
const OFFSET: Param = Param::new("offset", ParamType::Mixed)
    .by_ref()
    .with_default(ConstantValue::Null);

const INIT: &[Param] = &[
    Param::new("locale", ParamType::String),
    Param::new("style", ParamType::Int),
    Param::new("pattern", ParamType::String)
        .nullable()
        .with_default(ConstantValue::Null),
];

const FORMAT: &[Param] = &[Param::new("num", ParamType::Number), TYPE_DEFAULT];
const FN_FORMAT: &[Param] = &[CLASS, Param::new("num", ParamType::Number), TYPE_DEFAULT];
const CURRENCY: &[Param] = &[
    Param::new("amount", ParamType::Float),
    Param::new("currency", ParamType::String),
];
const FN_CURRENCY: &[Param] = &[
    CLASS,
    Param::new("amount", ParamType::Float),
    Param::new("currency", ParamType::String),
];
const PARSE: &[Param] = &[STRING, TYPE_DOUBLE, OFFSET];
const FN_PARSE: &[Param] = &[CLASS, STRING, TYPE_DOUBLE, OFFSET];
const PARSE_CURRENCY: &[Param] = &[
    STRING,
    Param::new("currency", ParamType::Mixed).by_ref(),
    OFFSET,
];
const FN_PARSE_CURRENCY: &[Param] = &[
    CLASS,
    STRING,
    Param::new("currency", ParamType::Mixed).by_ref(),
    OFFSET,
];
const ATTR: &[Param] = &[ATTRIBUTE];
const FN_ATTR: &[Param] = &[CLASS, ATTRIBUTE];
const SET_ATTR: &[Param] = &[ATTRIBUTE, Param::new("value", ParamType::Number)];
const FN_SET_ATTR: &[Param] = &[CLASS, ATTRIBUTE, Param::new("value", ParamType::Number)];
const SET_TEXT: &[Param] = &[ATTRIBUTE, VALUE];
const FN_SET_TEXT: &[Param] = &[CLASS, ATTRIBUTE, VALUE];
const SYMBOL: &[Param] = &[SYMBOL_ARG];
const FN_SYMBOL: &[Param] = &[CLASS, SYMBOL_ARG];
const SET_SYMBOL: &[Param] = &[SYMBOL_ARG, VALUE];
const FN_SET_SYMBOL: &[Param] = &[CLASS, SYMBOL_ARG, VALUE];
const PATTERN: &[Param] = &[Param::new("pattern", ParamType::String)];
const FN_PATTERN: &[Param] = &[CLASS, Param::new("pattern", ParamType::String)];
const LOCALE_QUERY: &[Param] = &[Param::new("type", ParamType::Int)
    .with_constant_default("ULOC_ACTUAL_LOCALE", ConstantValue::Int(0))];
const FN_LOCALE_QUERY: &[Param] = &[
    CLASS,
    Param::new("type", ParamType::Int)
        .with_constant_default("ULOC_ACTUAL_LOCALE", ConstantValue::Int(0)),
];
const FN_NONE: &[Param] = &[CLASS];

const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::copy::<Formatter>()),
    fork: Some(phera_extension_api::fork_clone::<Formatter>),
    ..ClassHooks::EMPTY
};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        constants: CONSTANTS,
        hooks: Some(&HOOKS),
        methods: const {
            &[
                MethodEntry::public("__construct", INIT, |c, o, a| {
                    init(c, o, a, "NumberFormatter::__construct")
                }),
                MethodEntry::public("create", INIT, |c, _, a| {
                    init(c, None, a, "NumberFormatter::create")
                })
                .static_()
                .tentative_return("?NumberFormatter"),
                MethodEntry::public("format", FORMAT, |c, o, a| {
                    dispatch(c, o, a, "NumberFormatter::format", Operation::Format)
                })
                .tentative_return("string|false"),
                MethodEntry::public("formatCurrency", CURRENCY, |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "NumberFormatter::formatCurrency",
                        Operation::FormatCurrency,
                    )
                })
                .tentative_return("string|false"),
                MethodEntry::public("parse", PARSE, |c, o, a| {
                    dispatch(c, o, a, "NumberFormatter::parse", Operation::Parse)
                })
                .tentative_return("int|float|false"),
                MethodEntry::public("parseCurrency", PARSE_CURRENCY, |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "NumberFormatter::parseCurrency",
                        Operation::ParseCurrency,
                    )
                })
                .tentative_return("float|false"),
                MethodEntry::public("getAttribute", ATTR, |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "NumberFormatter::getAttribute",
                        Operation::GetAttribute,
                    )
                })
                .tentative_return("int|float|false"),
                MethodEntry::public("setAttribute", SET_ATTR, |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "NumberFormatter::setAttribute",
                        Operation::SetAttribute,
                    )
                })
                .tentative_return("bool"),
                MethodEntry::public("getTextAttribute", ATTR, |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "NumberFormatter::getTextAttribute",
                        Operation::GetText,
                    )
                })
                .tentative_return("string|false"),
                MethodEntry::public("setTextAttribute", SET_TEXT, |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "NumberFormatter::setTextAttribute",
                        Operation::SetText,
                    )
                })
                .tentative_return("bool"),
                MethodEntry::public("getSymbol", SYMBOL, |c, o, a| {
                    dispatch(c, o, a, "NumberFormatter::getSymbol", Operation::GetSymbol)
                })
                .tentative_return("string|false"),
                MethodEntry::public("setSymbol", SET_SYMBOL, |c, o, a| {
                    dispatch(c, o, a, "NumberFormatter::setSymbol", Operation::SetSymbol)
                })
                .tentative_return("bool"),
                MethodEntry::public("getPattern", &[], |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "NumberFormatter::getPattern",
                        Operation::GetPattern,
                    )
                })
                .tentative_return("string|false"),
                MethodEntry::public("setPattern", PATTERN, |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "NumberFormatter::setPattern",
                        Operation::SetPattern,
                    )
                })
                .tentative_return("bool"),
                MethodEntry::public("getLocale", LOCALE_QUERY, |c, o, a| {
                    dispatch(c, o, a, "NumberFormatter::getLocale", Operation::Locale)
                })
                .tentative_return("string|false"),
                MethodEntry::public("getErrorCode", &[], |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "NumberFormatter::getErrorCode",
                        Operation::ErrorCode,
                    )
                })
                .tentative_return("int"),
                MethodEntry::public("getErrorMessage", &[], |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "NumberFormatter::getErrorMessage",
                        Operation::ErrorMessage,
                    )
                })
                .tentative_return("string"),
            ]
        },
        ..ClassEntry::class("NumberFormatter")
    });
    for (entry, result) in [
        (
            FunctionEntry::sync("numfmt_create", INIT, |c, a| {
                init(c, None, a, "numfmt_create")
            }),
            "?NumberFormatter",
        ),
        (
            FunctionEntry::sync("numfmt_format", FN_FORMAT, |c, a| {
                dispatch(c, None, a, "numfmt_format", Operation::Format)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("numfmt_format_currency", FN_CURRENCY, |c, a| {
                dispatch(
                    c,
                    None,
                    a,
                    "numfmt_format_currency",
                    Operation::FormatCurrency,
                )
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("numfmt_parse", FN_PARSE, |c, a| {
                dispatch(c, None, a, "numfmt_parse", Operation::Parse)
            }),
            "int|float|false",
        ),
        (
            FunctionEntry::sync("numfmt_parse_currency", FN_PARSE_CURRENCY, |c, a| {
                dispatch(
                    c,
                    None,
                    a,
                    "numfmt_parse_currency",
                    Operation::ParseCurrency,
                )
            }),
            "float|false",
        ),
        (
            FunctionEntry::sync("numfmt_get_attribute", FN_ATTR, |c, a| {
                dispatch(c, None, a, "numfmt_get_attribute", Operation::GetAttribute)
            }),
            "int|float|false",
        ),
        (
            FunctionEntry::sync("numfmt_set_attribute", FN_SET_ATTR, |c, a| {
                dispatch(c, None, a, "numfmt_set_attribute", Operation::SetAttribute)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("numfmt_get_text_attribute", FN_ATTR, |c, a| {
                dispatch(c, None, a, "numfmt_get_text_attribute", Operation::GetText)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("numfmt_set_text_attribute", FN_SET_TEXT, |c, a| {
                dispatch(c, None, a, "numfmt_set_text_attribute", Operation::SetText)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("numfmt_get_symbol", FN_SYMBOL, |c, a| {
                dispatch(c, None, a, "numfmt_get_symbol", Operation::GetSymbol)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("numfmt_set_symbol", FN_SET_SYMBOL, |c, a| {
                dispatch(c, None, a, "numfmt_set_symbol", Operation::SetSymbol)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("numfmt_get_pattern", FN_NONE, |c, a| {
                dispatch(c, None, a, "numfmt_get_pattern", Operation::GetPattern)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("numfmt_set_pattern", FN_PATTERN, |c, a| {
                dispatch(c, None, a, "numfmt_set_pattern", Operation::SetPattern)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("numfmt_get_locale", FN_LOCALE_QUERY, |c, a| {
                dispatch(c, None, a, "numfmt_get_locale", Operation::Locale)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("numfmt_get_error_code", FN_NONE, |c, a| {
                dispatch(c, None, a, "numfmt_get_error_code", Operation::ErrorCode)
            }),
            "int",
        ),
        (
            FunctionEntry::sync("numfmt_get_error_message", FN_NONE, |c, a| {
                dispatch(
                    c,
                    None,
                    a,
                    "numfmt_get_error_message",
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
