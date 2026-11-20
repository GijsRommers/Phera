//! Common character operations with PHP's code-point unions and defaults.

use phera_extension_api::{ArrayKey, PhpArray, Value};
use phera_extension_api::{ClassEntry, ConstantValue, MethodEntry, Param, ParamType, Registry};

use super::{CLASS, Op, invoke};
use crate::unsupported;

const CODE: Param = Param::new("codepoint", ParamType::IntOrString);
const CODEPOINT: &[Param] = &[CODE];
const CHARACTER: &[Param] = &[Param::new("character", ParamType::IntOrString)];
const BASE: Param = Param::new("base", ParamType::Int).with_default(ConstantValue::Int(10));
const DIGIT: &[Param] = &[CODE, BASE];
const FOR_DIGIT: &[Param] = &[Param::new("digit", ParamType::Int), BASE];
const FOLD: &[Param] = &[
    CODE,
    Param::new("options", ParamType::Int)
        .with_constant_default("IntlChar::FOLD_CASE_DEFAULT", ConstantValue::Int(0)),
];
const PROPERTY_ID: Param = Param::new("property", ParamType::Int);
const PROPERTY: &[Param] = &[CODE, PROPERTY_ID];
const NAME: Param = Param::new("name", ParamType::String);
const NAME_TYPE: Param = Param::new("type", ParamType::Int).optional();
const CALLBACK: Param = Param::new("callback", ParamType::Callable);
const CHARACTER_RANGE: &[Param] = &[
    Param::new("start", ParamType::IntOrString),
    Param::new("end", ParamType::IntOrString),
    CALLBACK,
    NAME_TYPE,
];

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        constants: CONSTANTS,
        methods: METHODS,
        ..ClassEntry::class(CLASS)
    });
}

const CONSTANTS: &[(&str, ConstantValue)] = &[
    ("UNICODE_VERSION", ConstantValue::String("18.0")),
    ("CODEPOINT_MIN", ConstantValue::Int(0)),
    ("CODEPOINT_MAX", ConstantValue::Int(1114111)),
    ("NO_NUMERIC_VALUE", ConstantValue::Float(-123456789.0)),
    ("UNICODE_CHAR_NAME", ConstantValue::Int(0)),
    ("LONG_PROPERTY_NAME", ConstantValue::Int(1)),
    ("PROPERTY_ALPHABETIC", ConstantValue::Int(0)),
    ("PROPERTY_ASCII_HEX_DIGIT", ConstantValue::Int(1)),
    ("PROPERTY_BIDI_MIRRORED", ConstantValue::Int(3)),
    ("PROPERTY_HEX_DIGIT", ConstantValue::Int(13)),
    ("PROPERTY_ID_CONTINUE", ConstantValue::Int(15)),
    ("PROPERTY_ID_START", ConstantValue::Int(16)),
    ("PROPERTY_LOWERCASE", ConstantValue::Int(22)),
    ("PROPERTY_UPPERCASE", ConstantValue::Int(30)),
    ("PROPERTY_WHITE_SPACE", ConstantValue::Int(31)),
    (
        "PROPERTY_CANONICAL_COMBINING_CLASS",
        ConstantValue::Int(4098),
    ),
    ("PROPERTY_GENERAL_CATEGORY", ConstantValue::Int(4101)),
    ("CHAR_CATEGORY_UNASSIGNED", ConstantValue::Int(0)),
    ("CHAR_CATEGORY_GENERAL_OTHER_TYPES", ConstantValue::Int(0)),
    ("CHAR_CATEGORY_UPPERCASE_LETTER", ConstantValue::Int(1)),
    ("CHAR_CATEGORY_LOWERCASE_LETTER", ConstantValue::Int(2)),
    ("CHAR_CATEGORY_TITLECASE_LETTER", ConstantValue::Int(3)),
    ("CHAR_CATEGORY_MODIFIER_LETTER", ConstantValue::Int(4)),
    ("CHAR_CATEGORY_OTHER_LETTER", ConstantValue::Int(5)),
    ("CHAR_CATEGORY_NON_SPACING_MARK", ConstantValue::Int(6)),
    ("CHAR_CATEGORY_ENCLOSING_MARK", ConstantValue::Int(7)),
    (
        "CHAR_CATEGORY_COMBINING_SPACING_MARK",
        ConstantValue::Int(8),
    ),
    ("CHAR_CATEGORY_DECIMAL_DIGIT_NUMBER", ConstantValue::Int(9)),
    ("CHAR_CATEGORY_LETTER_NUMBER", ConstantValue::Int(10)),
    ("CHAR_CATEGORY_OTHER_NUMBER", ConstantValue::Int(11)),
    ("CHAR_CATEGORY_SPACE_SEPARATOR", ConstantValue::Int(12)),
    ("CHAR_CATEGORY_LINE_SEPARATOR", ConstantValue::Int(13)),
    ("CHAR_CATEGORY_PARAGRAPH_SEPARATOR", ConstantValue::Int(14)),
    ("CHAR_CATEGORY_CONTROL_CHAR", ConstantValue::Int(15)),
    ("CHAR_CATEGORY_FORMAT_CHAR", ConstantValue::Int(16)),
    ("CHAR_CATEGORY_PRIVATE_USE_CHAR", ConstantValue::Int(17)),
    ("CHAR_CATEGORY_SURROGATE", ConstantValue::Int(18)),
    ("CHAR_CATEGORY_DASH_PUNCTUATION", ConstantValue::Int(19)),
    ("CHAR_CATEGORY_START_PUNCTUATION", ConstantValue::Int(20)),
    ("CHAR_CATEGORY_END_PUNCTUATION", ConstantValue::Int(21)),
    (
        "CHAR_CATEGORY_CONNECTOR_PUNCTUATION",
        ConstantValue::Int(22),
    ),
    ("CHAR_CATEGORY_OTHER_PUNCTUATION", ConstantValue::Int(23)),
    ("CHAR_CATEGORY_MATH_SYMBOL", ConstantValue::Int(24)),
    ("CHAR_CATEGORY_CURRENCY_SYMBOL", ConstantValue::Int(25)),
    ("CHAR_CATEGORY_MODIFIER_SYMBOL", ConstantValue::Int(26)),
    ("CHAR_CATEGORY_OTHER_SYMBOL", ConstantValue::Int(27)),
    ("CHAR_CATEGORY_INITIAL_PUNCTUATION", ConstantValue::Int(28)),
    ("CHAR_CATEGORY_FINAL_PUNCTUATION", ConstantValue::Int(29)),
    ("CHAR_CATEGORY_CHAR_CATEGORY_COUNT", ConstantValue::Int(30)),
    ("FOLD_CASE_DEFAULT", ConstantValue::Int(0)),
    ("FOLD_CASE_EXCLUDE_SPECIAL_I", ConstantValue::Int(1)),
];

const METHODS: &[MethodEntry] = &[
    MethodEntry::public("getUnicodeVersion", &[], |_, _, _| {
        let values: PhpArray = [18, 0, 0, 0]
            .into_iter()
            .enumerate()
            .map(|(index, value)| (ArrayKey::Int(index as i64), Value::Int(value)))
            .collect();
        Ok(Value::from(values))
    })
    .static_()
    .returning("array"),
    MethodEntry::public("ord", CHARACTER, |cx, _, values| {
        invoke(cx, values, "IntlChar::ord", Op::Ord)
    })
    .static_()
    .returning("?int"),
    MethodEntry::public("chr", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::chr", Op::Chr)
    })
    .static_()
    .returning("?string"),
    MethodEntry::public("tolower", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::tolower", Op::Lower)
    })
    .static_()
    .returning("int|string|null"),
    MethodEntry::public("toupper", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::toupper", Op::Upper)
    })
    .static_()
    .returning("int|string|null"),
    MethodEntry::public("totitle", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::totitle", Op::Title)
    })
    .static_()
    .returning("int|string|null"),
    MethodEntry::public("foldCase", FOLD, |cx, _, values| {
        invoke(cx, values, "IntlChar::foldCase", Op::Fold)
    })
    .static_()
    .returning("int|string|null"),
    MethodEntry::public("charType", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::charType", Op::Type)
    })
    .static_()
    .returning("?int"),
    MethodEntry::public("getCombiningClass", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::getCombiningClass", Op::Combining)
    })
    .static_()
    .returning("?int"),
    MethodEntry::public("charDigitValue", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::charDigitValue", Op::DigitValue)
    })
    .static_()
    .returning("?int"),
    MethodEntry::public("digit", DIGIT, |cx, _, values| {
        invoke(cx, values, "IntlChar::digit", Op::Digit)
    })
    .static_()
    .returning("int|false|null"),
    MethodEntry::public("forDigit", FOR_DIGIT, |cx, _, values| {
        invoke(cx, values, "IntlChar::forDigit", Op::ForDigit)
    })
    .static_()
    .returning("int"),
    MethodEntry::public("hasBinaryProperty", PROPERTY, |cx, _, values| {
        invoke(cx, values, "IntlChar::hasBinaryProperty", Op::Binary)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("getIntPropertyValue", PROPERTY, |cx, _, values| {
        invoke(cx, values, "IntlChar::getIntPropertyValue", Op::IntProperty)
    })
    .static_()
    .returning("?int"),
    MethodEntry::public("isalnum", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isalnum", Op::Alnum)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isalpha", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isalpha", Op::Alpha)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isbase", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isbase", Op::Base)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isblank", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isblank", Op::Blank)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("iscntrl", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::iscntrl", Op::Control)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isdefined", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isdefined", Op::Defined)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isdigit", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isdigit", Op::Decimal)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isgraph", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isgraph", Op::Graph)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isIDIgnorable", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isIDIgnorable", Op::IdIgnore)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isIDPart", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isIDPart", Op::IdPart)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isIDStart", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isIDStart", Op::IdStart)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isISOControl", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isISOControl", Op::IsoControl)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isJavaIDPart", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isJavaIDPart", Op::JavaIdPart)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isJavaIDStart", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isJavaIDStart", Op::JavaIdStart)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isJavaSpaceChar", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isJavaSpaceChar", Op::JavaSpace)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("islower", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::islower", Op::Lowercase)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isMirrored", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isMirrored", Op::Mirrored)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isprint", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isprint", Op::Print)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("ispunct", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::ispunct", Op::Punct)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isspace", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isspace", Op::Space)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("istitle", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::istitle", Op::Titlecase)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isUAlphabetic", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isUAlphabetic", Op::UAlphabetic)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isULowercase", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isULowercase", Op::ULowercase)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isupper", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isupper", Op::Uppercase)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isUUppercase", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isUUppercase", Op::UUppercase)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isUWhiteSpace", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isUWhiteSpace", Op::UWhiteSpace)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isWhitespace", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isWhitespace", Op::Whitespace)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("isxdigit", CODEPOINT, |cx, _, values| {
        invoke(cx, values, "IntlChar::isxdigit", Op::Xdigit)
    })
    .static_()
    .returning("?bool"),
    MethodEntry::public("charAge", CODEPOINT, |_, _, _| {
        Err(unsupported::item("IntlChar::charAge"))
    })
    .static_(),
    MethodEntry::public("charDirection", CODEPOINT, |_, _, _| {
        Err(unsupported::item("IntlChar::charDirection"))
    })
    .static_(),
    MethodEntry::public("charFromName", &[NAME, NAME_TYPE], |_, _, _| {
        Err(unsupported::item("IntlChar::charFromName"))
    })
    .static_(),
    MethodEntry::public("charMirror", CODEPOINT, |_, _, _| {
        Err(unsupported::item("IntlChar::charMirror"))
    })
    .static_(),
    MethodEntry::public("charName", &[CODE, NAME_TYPE], |_, _, _| {
        Err(unsupported::item("IntlChar::charName"))
    })
    .static_(),
    MethodEntry::public("enumCharNames", CHARACTER_RANGE, |_, _, _| {
        Err(unsupported::item("IntlChar::enumCharNames"))
    })
    .static_(),
    MethodEntry::public("enumCharTypes", &[CALLBACK], |_, _, _| {
        Err(unsupported::item("IntlChar::enumCharTypes"))
    })
    .static_(),
    MethodEntry::public("getBidiPairedBracket", CODEPOINT, |_, _, _| {
        Err(unsupported::item("IntlChar::getBidiPairedBracket"))
    })
    .static_(),
    MethodEntry::public("getBlockCode", CODEPOINT, |_, _, _| {
        Err(unsupported::item("IntlChar::getBlockCode"))
    })
    .static_(),
    MethodEntry::public("getFC_NFKC_Closure", CODEPOINT, |_, _, _| {
        Err(unsupported::item("IntlChar::getFC_NFKC_Closure"))
    })
    .static_(),
    MethodEntry::public("getIntPropertyMaxValue", &[PROPERTY_ID], |_, _, _| {
        Err(unsupported::item("IntlChar::getIntPropertyMaxValue"))
    })
    .static_(),
    MethodEntry::public("getIntPropertyMinValue", &[PROPERTY_ID], |_, _, _| {
        Err(unsupported::item("IntlChar::getIntPropertyMinValue"))
    })
    .static_(),
    MethodEntry::public("getNumericValue", CODEPOINT, |_, _, _| {
        Err(unsupported::item("IntlChar::getNumericValue"))
    })
    .static_(),
    MethodEntry::public(
        "getPropertyEnum",
        &[Param::new("alias", ParamType::String)],
        |_, _, _| Err(unsupported::item("IntlChar::getPropertyEnum")),
    )
    .static_(),
    MethodEntry::public("getPropertyName", &[PROPERTY_ID, NAME_TYPE], |_, _, _| {
        Err(unsupported::item("IntlChar::getPropertyName"))
    })
    .static_(),
    MethodEntry::public("getPropertyValueEnum", &[PROPERTY_ID, NAME], |_, _, _| {
        Err(unsupported::item("IntlChar::getPropertyValueEnum"))
    })
    .static_(),
    MethodEntry::public(
        "getPropertyValueName",
        &[PROPERTY_ID, Param::new("value", ParamType::Int), NAME_TYPE],
        |_, _, _| Err(unsupported::item("IntlChar::getPropertyValueName")),
    )
    .static_(),
];
