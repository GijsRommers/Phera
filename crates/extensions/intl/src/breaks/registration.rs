//! Register PHP's base and concrete break iterator classes.

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue, MethodEntry, NativeClone, Param, ParamType, Registry,
    Value, Visibility,
};

use super::{CLASS, CODE_POINT, Mode, Op, RULES, State, create, cursor, invoke, parts};
use crate::unsupported;

const LOCALE: &[Param] = &[Param::new("locale", ParamType::String)
    .nullable()
    .with_default(ConstantValue::Null)];
const OFFSET: &[Param] = &[Param::new("offset", ParamType::Int)];
const NEXT: &[Param] = &[Param::new("offset", ParamType::Int)
    .nullable()
    .with_default(ConstantValue::Null)];
const TEXT: &[Param] = &[Param::new("text", ParamType::String)];
const LOCALE_TYPE: &[Param] = &[Param::new("type", ParamType::Int)];
const PARTS: &[Param] = &[Param::new("type", ParamType::String)
    .with_constant_default("IntlPartsIterator::KEY_SEQUENTIAL", ConstantValue::Int(0))];
const RULE_ARGS: &[Param] = &[
    Param::new("rules", ParamType::String),
    Param::new("compiled", ParamType::Bool).with_default(ConstantValue::Bool(false)),
];
const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::copy::<State>()),
    fork: Some(phera_extension_api::fork_clone::<State>),
    iterator: Some(cursor::make),
    ..ClassHooks::EMPTY
};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["IteratorAggregate"],
        hooks: Some(&HOOKS),
        constants: &[
            ("DONE", ConstantValue::Int(-1)),
            ("WORD_NONE", ConstantValue::Int(0)),
            ("WORD_NONE_LIMIT", ConstantValue::Int(100)),
            ("WORD_NUMBER", ConstantValue::Int(100)),
            ("WORD_NUMBER_LIMIT", ConstantValue::Int(200)),
            ("WORD_LETTER", ConstantValue::Int(200)),
            ("WORD_LETTER_LIMIT", ConstantValue::Int(300)),
            ("WORD_KANA", ConstantValue::Int(300)),
            ("WORD_KANA_LIMIT", ConstantValue::Int(400)),
            ("WORD_IDEO", ConstantValue::Int(400)),
            ("WORD_IDEO_LIMIT", ConstantValue::Int(500)),
            ("LINE_SOFT", ConstantValue::Int(0)),
            ("LINE_SOFT_LIMIT", ConstantValue::Int(100)),
            ("LINE_HARD", ConstantValue::Int(100)),
            ("LINE_HARD_LIMIT", ConstantValue::Int(200)),
            ("SENTENCE_TERM", ConstantValue::Int(0)),
            ("SENTENCE_TERM_LIMIT", ConstantValue::Int(100)),
            ("SENTENCE_SEP", ConstantValue::Int(100)),
            ("SENTENCE_SEP_LIMIT", ConstantValue::Int(200)),
        ],
        methods: const {
            &[
                MethodEntry::public("__construct", &[], |_, _, _| Ok(Value::Null))
                    .with_visibility(Visibility::Private),
                MethodEntry::public("createTitleInstance", LOCALE, |_, _, _| {
                    Err(unsupported::item("IntlBreakIterator::createTitleInstance"))
                })
                .static_()
                .returning("?IntlBreakIterator"),
                MethodEntry::public("createCharacterInstance", LOCALE, |cx, _, args| {
                    create(cx, args, Mode::Character)
                })
                .static_()
                .returning("?IntlBreakIterator"),
                MethodEntry::public("createCodePointInstance", &[], |cx, _, args| {
                    create(cx, args, Mode::CodePoint)
                })
                .static_()
                .returning("IntlCodePointBreakIterator"),
                MethodEntry::public("createLineInstance", LOCALE, |cx, _, args| {
                    create(cx, args, Mode::Line)
                })
                .static_()
                .returning("?IntlBreakIterator"),
                MethodEntry::public("createSentenceInstance", LOCALE, |cx, _, args| {
                    create(cx, args, Mode::Sentence)
                })
                .static_()
                .returning("?IntlBreakIterator"),
                MethodEntry::public("createWordInstance", LOCALE, |cx, _, args| {
                    create(cx, args, Mode::Word)
                })
                .static_()
                .returning("?IntlBreakIterator"),
                MethodEntry::public("current", &[], |cx, object, args| {
                    invoke(cx, object, args, Op::Current)
                })
                .returning("int"),
                MethodEntry::public("first", &[], |cx, object, args| {
                    invoke(cx, object, args, Op::First)
                })
                .returning("int"),
                MethodEntry::public("last", &[], |cx, object, args| {
                    invoke(cx, object, args, Op::Last)
                })
                .returning("int"),
                MethodEntry::public("next", NEXT, |cx, object, args| {
                    invoke(cx, object, args, Op::Next)
                })
                .returning("int"),
                MethodEntry::public("previous", &[], |cx, object, args| {
                    invoke(cx, object, args, Op::Previous)
                })
                .returning("int"),
                MethodEntry::public("following", OFFSET, |cx, object, args| {
                    invoke(cx, object, args, Op::Following)
                })
                .returning("int"),
                MethodEntry::public("preceding", OFFSET, |cx, object, args| {
                    invoke(cx, object, args, Op::Preceding)
                })
                .returning("int"),
                MethodEntry::public("isBoundary", OFFSET, |cx, object, args| {
                    invoke(cx, object, args, Op::Boundary)
                })
                .returning("bool"),
                MethodEntry::public("getText", &[], |cx, object, args| {
                    invoke(cx, object, args, Op::GetText)
                })
                .returning("?string"),
                MethodEntry::public("setText", TEXT, |cx, object, args| {
                    invoke(cx, object, args, Op::SetText)
                })
                .returning("bool"),
                MethodEntry::public("getLocale", LOCALE_TYPE, |cx, object, args| {
                    invoke(cx, object, args, Op::Locale)
                })
                .returning("string|false"),
                MethodEntry::public("getErrorCode", &[], |cx, object, args| {
                    invoke(cx, object, args, Op::ErrorCode)
                })
                .returning("int"),
                MethodEntry::public("getErrorMessage", &[], |cx, object, args| {
                    invoke(cx, object, args, Op::ErrorMessage)
                })
                .returning("string"),
                MethodEntry::public("getIterator", &[], |cx, object, _| {
                    cursor::wrapping(cx, object)
                })
                .returning("Iterator"),
                MethodEntry::public("getPartsIterator", PARTS, parts)
                    .returning("IntlPartsIterator"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
    registry.class(ClassEntry {
        parent: Some(CLASS),
        hooks: Some(&HOOKS),
        methods: const {
            &[
                MethodEntry::public("__construct", RULE_ARGS, |_, _, _| {
                    Err(unsupported::item("IntlRuleBasedBreakIterator::__construct"))
                }),
                MethodEntry::public("getRules", &[], |_, _, _| {
                    Err(unsupported::item("IntlRuleBasedBreakIterator::getRules"))
                })
                .returning("string|false"),
                MethodEntry::public("getBinaryRules", &[], |_, _, _| {
                    Err(unsupported::item(
                        "IntlRuleBasedBreakIterator::getBinaryRules",
                    ))
                })
                .returning("string|false"),
                MethodEntry::public("getRuleStatus", &[], |cx, object, args| {
                    invoke(cx, object, args, Op::Status)
                })
                .returning("int"),
                MethodEntry::public("getRuleStatusVec", &[], |cx, object, args| {
                    invoke(cx, object, args, Op::StatusVec)
                })
                .returning("array|false"),
            ]
        },
        ..ClassEntry::class(RULES)
    });
    registry.class(ClassEntry {
        parent: Some(CLASS),
        hooks: Some(&HOOKS),
        methods: const {
            &[
                MethodEntry::public("getLastCodePoint", &[], |cx, object, args| {
                    invoke(cx, object, args, Op::LastCodePoint)
                })
                .returning("int"),
            ]
        },
        ..ClassEntry::class(CODE_POINT)
    });
}
