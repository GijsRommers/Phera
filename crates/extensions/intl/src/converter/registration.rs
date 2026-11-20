//! Register UConverter's PHP names and tentative return types.
//!
//! ```php
//! UConverter::reasonText(UConverter::REASON_ILLEGAL); // "REASON_ILLEGAL"
//! ```

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, ConstantValue, MethodEntry, NativeClone, NativeError, Param,
    ParamType, PhpArray, Registry, Value, args,
};

use super::{CLASS, Converter, Op, construct, dispatch, encoding, transcode};
use crate::{error, unsupported};

const ENCODINGS: &[Param] = &[
    Param::new("destination_encoding", ParamType::String)
        .nullable()
        .optional(),
    Param::new("source_encoding", ParamType::String)
        .nullable()
        .optional(),
];
const TEXT: Param = Param::new("str", ParamType::String);
const TRANSCODE: &[Param] = &[
    TEXT,
    Param::new("toEncoding", ParamType::String),
    Param::new("fromEncoding", ParamType::String),
    Param::new("options", ParamType::Array)
        .nullable()
        .optional(),
];
const CONVERT: &[Param] = &[TEXT, Param::new("reverse", ParamType::Bool).optional()];
const ENCODING: &[Param] = &[Param::new("encoding", ParamType::String)];
const CHARS: &[Param] = &[Param::new("chars", ParamType::String)];
const NAME: &[Param] = &[Param::new("name", ParamType::String)];
const REASON: Param = Param::new("reason", ParamType::Int);
const ERROR: Param = Param::new("error", ParamType::Mixed).by_ref();
const REASONS: &[&str] = &[
    "REASON_UNASSIGNED",
    "REASON_ILLEGAL",
    "REASON_IRREGULAR",
    "REASON_RESET",
    "REASON_CLOSE",
    "REASON_CLONE",
];
const STANDARDS: &[&str] = &["UTR22", "IBM", "WINDOWS", "JAVA", "IANA", "MIME", ""];
const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::copy::<Converter>()),
    ..ClassHooks::EMPTY
};

pub(super) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        hooks: Some(&HOOKS),
        constants: &[
            ("REASON_UNASSIGNED", ConstantValue::Int(0)),
            ("REASON_ILLEGAL", ConstantValue::Int(1)),
            ("REASON_IRREGULAR", ConstantValue::Int(2)),
            ("REASON_RESET", ConstantValue::Int(3)),
            ("REASON_CLOSE", ConstantValue::Int(4)),
            ("REASON_CLONE", ConstantValue::Int(5)),
            ("SBCS", ConstantValue::Int(0)),
            ("LATIN_1", ConstantValue::Int(3)),
            ("UTF8", ConstantValue::Int(4)),
            ("UTF16_BigEndian", ConstantValue::Int(5)),
            ("UTF16_LittleEndian", ConstantValue::Int(6)),
            ("UTF32_BigEndian", ConstantValue::Int(7)),
            ("UTF32_LittleEndian", ConstantValue::Int(8)),
            ("US_ASCII", ConstantValue::Int(26)),
            ("UTF16", ConstantValue::Int(29)),
            ("UTF32", ConstantValue::Int(30)),
        ],
        methods: METHODS,
        ..ClassEntry::class(CLASS)
    });
}

const METHODS: &[MethodEntry] = &[
    MethodEntry::public("__construct", ENCODINGS, construct),
    MethodEntry::public("transcode", TRANSCODE, |cx, _, args| transcode(cx, args))
        .static_()
        .returning("string|false"),
    MethodEntry::public("convert", CONVERT, |cx, object, args| {
        dispatch(cx, object, args, "UConverter::convert", Op::Convert)
    })
    .returning("string|false"),
    MethodEntry::public("getSourceEncoding", &[], |cx, object, args| {
        dispatch(
            cx,
            object,
            args,
            "UConverter::getSourceEncoding",
            Op::Source,
        )
    })
    .returning("string|false|null"),
    MethodEntry::public("getSourceType", &[], |cx, object, args| {
        dispatch(
            cx,
            object,
            args,
            "UConverter::getSourceType",
            Op::SourceType,
        )
    })
    .returning("int|false|null"),
    MethodEntry::public("setSourceEncoding", ENCODING, |cx, object, args| {
        dispatch(
            cx,
            object,
            args,
            "UConverter::setSourceEncoding",
            Op::SetSource,
        )
    })
    .returning("bool"),
    MethodEntry::public("getDestinationEncoding", &[], |cx, object, args| {
        dispatch(
            cx,
            object,
            args,
            "UConverter::getDestinationEncoding",
            Op::Destination,
        )
    })
    .returning("string|false|null"),
    MethodEntry::public("getDestinationType", &[], |cx, object, args| {
        dispatch(
            cx,
            object,
            args,
            "UConverter::getDestinationType",
            Op::DestinationType,
        )
    })
    .returning("int|false|null"),
    MethodEntry::public("setDestinationEncoding", ENCODING, |cx, object, args| {
        dispatch(
            cx,
            object,
            args,
            "UConverter::setDestinationEncoding",
            Op::SetDestination,
        )
    })
    .returning("bool"),
    MethodEntry::public("getSubstChars", &[], |cx, object, args| {
        dispatch(
            cx,
            object,
            args,
            "UConverter::getSubstChars",
            Op::Substitution,
        )
    })
    .returning("string|false|null"),
    MethodEntry::public("setSubstChars", CHARS, |cx, object, args| {
        dispatch(
            cx,
            object,
            args,
            "UConverter::setSubstChars",
            Op::SetSubstitution,
        )
    })
    .returning("bool"),
    MethodEntry::public("getErrorCode", &[], |cx, object, args| {
        dispatch(cx, object, args, "UConverter::getErrorCode", Op::ErrorCode)
    })
    .returning("int"),
    MethodEntry::public("getErrorMessage", &[], |cx, object, args| {
        dispatch(
            cx,
            object,
            args,
            "UConverter::getErrorMessage",
            Op::ErrorMessage,
        )
    })
    .returning("?string"),
    MethodEntry::public("getAliases", NAME, |cx, _, values| {
        error::clear(cx);
        let result: PhpArray = encoding::find(args::bytes(values, 0))
            .into_iter()
            .flat_map(|(encoding, _)| &encoding.aliases)
            .enumerate()
            .map(|(index, (alias, _))| (ArrayKey::Int(index as i64), Value::from(*alias)))
            .collect();
        Ok(Value::from(result))
    })
    .static_()
    .returning("array|false|null"),
    MethodEntry::public("getAvailable", &[], |_, _, _| {
        Err(unsupported::item("UConverter::getAvailable"))
    })
    .static_()
    .returning("array"),
    MethodEntry::public("getStandards", &[], |cx, _, _| {
        error::clear(cx);
        let result: PhpArray = STANDARDS
            .iter()
            .copied()
            .enumerate()
            .map(|(index, alias)| (ArrayKey::Int(index as i64), Value::from(alias)))
            .collect();
        Ok(Value::from(result))
    })
    .static_()
    .returning("?array"),
    MethodEntry::public("reasonText", &[REASON], |cx, _, values| {
        error::clear(cx);
        args::int(values, 0)
            .and_then(|reason| usize::try_from(reason).ok())
            .and_then(|reason| REASONS.get(reason))
            .map(|reason| Value::from(*reason))
            .ok_or_else(|| {
                NativeError::value_error(concat!(
                    "UConverter::reasonText(): Argument #1 ($reason) must be a ",
                    "UConverter::REASON_* constant"
                ))
            })
    })
    .static_()
    .returning("string"),
    MethodEntry::public(
        "fromUCallback",
        &[
            REASON,
            Param::new("source", ParamType::Array),
            Param::new("codePoint", ParamType::Int),
            ERROR,
        ],
        |_, _, _| Err(unsupported::item("UConverter::fromUCallback")),
    )
    .returning("string|int|array|null"),
    MethodEntry::public(
        "toUCallback",
        &[
            REASON,
            Param::new("source", ParamType::String),
            Param::new("codeUnits", ParamType::String),
            ERROR,
        ],
        |_, _, _| Err(unsupported::item("UConverter::toUCallback")),
    )
    .returning("string|int|array|null"),
];
