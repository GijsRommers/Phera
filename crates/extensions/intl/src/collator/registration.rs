//! PHP bindings for the Collator object and its procedural aliases.
//!
//! ```php
//! collator_compare($collator, 'x', 'y') === $collator->compare('x', 'y'); // true
//! ```

use phera_extension_api::{ClassEntry, ClassHooks, ConstantValue, FunctionEntry, MethodEntry};
use phera_extension_api::{NativeClone, Param, ParamType, Registry};

use super::{CLASS, Collator, Operation, construct, dispatch};

const OBJECT: Param = Param::new("object", ParamType::Class("Collator"));
const ARRAY: Param = Param::new("array", ParamType::Array).by_ref();
const ATTRIBUTE_ARG: Param = Param::new("attribute", ParamType::Int);
const FLAGS: Param = Param::new("flags", ParamType::Int)
    .with_constant_default("Collator::SORT_REGULAR", ConstantValue::Int(0));
const LOCALE: &[Param] = &[Param::new("locale", ParamType::String)];
const COMPARE: &[Param] = &[
    Param::new("string1", ParamType::String),
    Param::new("string2", ParamType::String),
];
const FN_COMPARE: &[Param] = &[
    OBJECT,
    Param::new("string1", ParamType::String),
    Param::new("string2", ParamType::String),
];
const SORT: &[Param] = &[ARRAY, FLAGS];
const FN_SORT: &[Param] = &[OBJECT, ARRAY, FLAGS];
const KEYS: &[Param] = &[ARRAY];
const FN_KEYS: &[Param] = &[OBJECT, ARRAY];
const ATTRIBUTE: &[Param] = &[ATTRIBUTE_ARG];
const FN_ATTRIBUTE: &[Param] = &[OBJECT, ATTRIBUTE_ARG];
const SET_ATTRIBUTE: &[Param] = &[ATTRIBUTE_ARG, Param::new("value", ParamType::Int)];
const FN_SET_ATTRIBUTE: &[Param] = &[OBJECT, ATTRIBUTE_ARG, Param::new("value", ParamType::Int)];
const STRENGTH: &[Param] = &[Param::new("strength", ParamType::Int)];
const FN_STRENGTH: &[Param] = &[OBJECT, Param::new("strength", ParamType::Int)];
const TYPE: &[Param] = &[Param::new("type", ParamType::Int)];
const FN_TYPE: &[Param] = &[OBJECT, Param::new("type", ParamType::Int)];
const SORT_KEY: &[Param] = &[Param::new("string", ParamType::String)];
const FN_SORT_KEY: &[Param] = &[OBJECT, Param::new("string", ParamType::String)];
const FN_NONE: &[Param] = &[OBJECT];
const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::copy::<Collator>()),
    fork: Some(phera_extension_api::fork_clone::<Collator>),
    ..ClassHooks::EMPTY
};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        hooks: Some(&HOOKS),
        constants: &[
            ("DEFAULT_VALUE", ConstantValue::Int(-1)),
            ("PRIMARY", ConstantValue::Int(0)),
            ("SECONDARY", ConstantValue::Int(1)),
            ("TERTIARY", ConstantValue::Int(2)),
            ("DEFAULT_STRENGTH", ConstantValue::Int(2)),
            ("QUATERNARY", ConstantValue::Int(3)),
            ("IDENTICAL", ConstantValue::Int(15)),
            ("OFF", ConstantValue::Int(16)),
            ("ON", ConstantValue::Int(17)),
            ("SHIFTED", ConstantValue::Int(20)),
            ("NON_IGNORABLE", ConstantValue::Int(21)),
            ("LOWER_FIRST", ConstantValue::Int(24)),
            ("UPPER_FIRST", ConstantValue::Int(25)),
            ("FRENCH_COLLATION", ConstantValue::Int(0)),
            ("ALTERNATE_HANDLING", ConstantValue::Int(1)),
            ("CASE_FIRST", ConstantValue::Int(2)),
            ("CASE_LEVEL", ConstantValue::Int(3)),
            ("NORMALIZATION_MODE", ConstantValue::Int(4)),
            ("STRENGTH", ConstantValue::Int(5)),
            ("HIRAGANA_QUATERNARY_MODE", ConstantValue::Int(6)),
            ("NUMERIC_COLLATION", ConstantValue::Int(7)),
            ("SORT_REGULAR", ConstantValue::Int(0)),
            ("SORT_STRING", ConstantValue::Int(1)),
            ("SORT_NUMERIC", ConstantValue::Int(2)),
        ],
        methods: const {
            &[
                MethodEntry::public("__construct", LOCALE, |c, o, a| {
                    construct(c, o, a, "Collator::__construct")
                }),
                MethodEntry::public("create", LOCALE, |c, _, a| {
                    construct(c, None, a, "Collator::create")
                })
                .static_()
                .tentative_return("?Collator"),
                MethodEntry::public("compare", COMPARE, |c, o, a| {
                    dispatch(c, o, a, "Collator::compare", Operation::Compare)
                })
                .tentative_return("int|false"),
                MethodEntry::public("sort", SORT, |c, o, a| {
                    dispatch(c, o, a, "Collator::sort", Operation::Sort)
                })
                .tentative_return("bool"),
                MethodEntry::public("sortWithSortKeys", KEYS, |c, o, a| {
                    dispatch(c, o, a, "Collator::sortWithSortKeys", Operation::SortKeys)
                })
                .tentative_return("bool"),
                MethodEntry::public("asort", SORT, |c, o, a| {
                    dispatch(c, o, a, "Collator::asort", Operation::Asort)
                })
                .tentative_return("bool"),
                MethodEntry::public("getAttribute", ATTRIBUTE, |c, o, a| {
                    dispatch(c, o, a, "Collator::getAttribute", Operation::GetAttr)
                })
                .tentative_return("int|false"),
                MethodEntry::public("setAttribute", SET_ATTRIBUTE, |c, o, a| {
                    dispatch(c, o, a, "Collator::setAttribute", Operation::SetAttr)
                })
                .tentative_return("bool"),
                MethodEntry::public("getStrength", &[], |c, o, a| {
                    dispatch(c, o, a, "Collator::getStrength", Operation::GetStrength)
                })
                .tentative_return("int"),
                MethodEntry::public("setStrength", STRENGTH, |c, o, a| {
                    dispatch(c, o, a, "Collator::setStrength", Operation::SetStrength)
                })
                .tentative_return("true"),
                MethodEntry::public("getLocale", TYPE, |c, o, a| {
                    dispatch(c, o, a, "Collator::getLocale", Operation::Locale)
                })
                .tentative_return("string|false"),
                MethodEntry::public("getErrorCode", &[], |c, o, a| {
                    dispatch(c, o, a, "Collator::getErrorCode", Operation::ErrorCode)
                })
                .tentative_return("int|false"),
                MethodEntry::public("getErrorMessage", &[], |c, o, a| {
                    dispatch(
                        c,
                        o,
                        a,
                        "Collator::getErrorMessage",
                        Operation::ErrorMessage,
                    )
                })
                .tentative_return("string|false"),
                MethodEntry::public("getSortKey", SORT_KEY, |c, o, a| {
                    dispatch(c, o, a, "Collator::getSortKey", Operation::SortKey)
                })
                .tentative_return("string|false"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
    for (entry, result) in [
        (
            FunctionEntry::sync("collator_create", LOCALE, |c, a| {
                construct(c, None, a, "collator_create")
            }),
            "?Collator",
        ),
        (
            FunctionEntry::sync("collator_compare", FN_COMPARE, |c, a| {
                dispatch(c, None, a, "collator_compare", Operation::Compare)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("collator_sort", FN_SORT, |c, a| {
                dispatch(c, None, a, "collator_sort", Operation::Sort)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("collator_sort_with_sort_keys", FN_KEYS, |c, a| {
                dispatch(
                    c,
                    None,
                    a,
                    "collator_sort_with_sort_keys",
                    Operation::SortKeys,
                )
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("collator_asort", FN_SORT, |c, a| {
                dispatch(c, None, a, "collator_asort", Operation::Asort)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("collator_get_attribute", FN_ATTRIBUTE, |c, a| {
                dispatch(c, None, a, "collator_get_attribute", Operation::GetAttr)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("collator_set_attribute", FN_SET_ATTRIBUTE, |c, a| {
                dispatch(c, None, a, "collator_set_attribute", Operation::SetAttr)
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("collator_get_strength", FN_NONE, |c, a| {
                dispatch(c, None, a, "collator_get_strength", Operation::GetStrength)
            }),
            "int",
        ),
        (
            FunctionEntry::sync("collator_set_strength", FN_STRENGTH, |c, a| {
                dispatch(c, None, a, "collator_set_strength", Operation::SetStrength)
            }),
            "true",
        ),
        (
            FunctionEntry::sync("collator_get_locale", FN_TYPE, |c, a| {
                dispatch(c, None, a, "collator_get_locale", Operation::Locale)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("collator_get_error_code", FN_NONE, |c, a| {
                dispatch(c, None, a, "collator_get_error_code", Operation::ErrorCode)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("collator_get_error_message", FN_NONE, |c, a| {
                dispatch(
                    c,
                    None,
                    a,
                    "collator_get_error_message",
                    Operation::ErrorMessage,
                )
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("collator_get_sort_key", FN_SORT_KEY, |c, a| {
                dispatch(c, None, a, "collator_get_sort_key", Operation::SortKey)
            }),
            "string|false",
        ),
    ] {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
}
