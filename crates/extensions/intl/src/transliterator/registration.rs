//! Native signatures for PHP's object and procedural transliteration APIs.

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, ConstantValue, FunctionEntry, MethodEntry, NativeClone,
    Param, ParamType, PhpArray, PropertyEntry, PropertyType, Registry, Value, Visibility,
};

use super::{CLASS, Op, State, create, dispatch, transform};
use crate::unsupported;

const OBJECT: Param = Param::new("transliterator", ParamType::Class(CLASS));
const DIRECTION: Param = Param::new("direction", ParamType::Int)
    .with_constant_default("Transliterator::FORWARD", ConstantValue::Int(0));
const CREATE: &[Param] = &[Param::new("id", ParamType::String), DIRECTION];
const RULES: &[Param] = &[Param::new("rules", ParamType::String), DIRECTION];
const STRING: Param = Param::new("string", ParamType::String);
const START: Param = Param::new("start", ParamType::Int).with_default(ConstantValue::Int(0));
const END: Param = Param::new("end", ParamType::Int).with_default(ConstantValue::Int(-1));
const APPLY: &[Param] = &[STRING, START, END];
const FN_APPLY: &[Param] = &[
    Param::new("transliterator", ParamType::ObjectOrString),
    STRING,
    START,
    END,
];
const INSTANCE: &[Param] = &[OBJECT];
const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::copy::<State>()),
    fork: Some(phera_extension_api::fork_clone::<State>),
    ..ClassHooks::EMPTY
};
const PROPERTIES: &[PropertyEntry] =
    &[PropertyEntry::typed("id", PropertyType::String, Visibility::Public).readonly()];

fn ids() -> Value {
    let values = transform::ids()
        .enumerate()
        .map(|(index, value)| (ArrayKey::Int(index as i64), Value::from(value)));
    Value::from(values.collect::<PhpArray>())
}

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        hooks: Some(&HOOKS),
        properties: PROPERTIES,
        constants: &[
            ("FORWARD", ConstantValue::Int(0)),
            ("REVERSE", ConstantValue::Int(1)),
        ],
        methods: const {
            &[
                MethodEntry::public("__construct", &[], |_, _, _| Ok(Value::Null))
                    .with_visibility(Visibility::Private)
                    .final_(),
                MethodEntry::public("create", CREATE, |cx, _, args| {
                    create(cx, args, "Transliterator::create")
                })
                .static_()
                .returning("?Transliterator"),
                MethodEntry::public("createFromRules", RULES, |_, _, _| {
                    Err(unsupported::item("Transliterator::createFromRules"))
                })
                .static_()
                .returning("?Transliterator"),
                MethodEntry::public("listIDs", &[], |_, _, _| Ok(ids()))
                    .static_()
                    .returning("array|false"),
                MethodEntry::public("createInverse", &[], |cx, object, args| {
                    dispatch(
                        cx,
                        object,
                        args,
                        "Transliterator::createInverse",
                        Op::Inverse,
                    )
                })
                .returning("?Transliterator"),
                MethodEntry::public("transliterate", APPLY, |cx, object, args| {
                    dispatch(
                        cx,
                        object,
                        args,
                        "Transliterator::transliterate",
                        Op::Transliterate,
                    )
                })
                .returning("string|false"),
                MethodEntry::public("getErrorCode", &[], |cx, object, args| {
                    dispatch(
                        cx,
                        object,
                        args,
                        "Transliterator::getErrorCode",
                        Op::ErrorCode,
                    )
                })
                .returning("int"),
                MethodEntry::public("getErrorMessage", &[], |cx, object, args| {
                    dispatch(
                        cx,
                        object,
                        args,
                        "Transliterator::getErrorMessage",
                        Op::ErrorMessage,
                    )
                })
                .returning("string"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
    for (entry, result) in [
        (
            FunctionEntry::sync("transliterator_create", CREATE, |cx, args| {
                create(cx, args, "transliterator_create")
            }),
            "?Transliterator",
        ),
        (
            FunctionEntry::sync("transliterator_create_from_rules", RULES, |_, _| {
                Err(unsupported::item("transliterator_create_from_rules"))
            }),
            "?Transliterator",
        ),
        (
            FunctionEntry::sync("transliterator_list_ids", &[], |_, _| Ok(ids())),
            "array|false",
        ),
        (
            FunctionEntry::sync("transliterator_create_inverse", INSTANCE, |cx, args| {
                dispatch(cx, None, args, "transliterator_create_inverse", Op::Inverse)
            }),
            "?Transliterator",
        ),
        (
            FunctionEntry::sync("transliterator_transliterate", FN_APPLY, |cx, args| {
                dispatch(
                    cx,
                    None,
                    args,
                    "transliterator_transliterate",
                    Op::Transliterate,
                )
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("transliterator_get_error_code", INSTANCE, |cx, args| {
                dispatch(
                    cx,
                    None,
                    args,
                    "transliterator_get_error_code",
                    Op::ErrorCode,
                )
            }),
            "int",
        ),
        (
            FunctionEntry::sync("transliterator_get_error_message", INSTANCE, |cx, args| {
                dispatch(
                    cx,
                    None,
                    args,
                    "transliterator_get_error_message",
                    Op::ErrorMessage,
                )
            }),
            "string",
        ),
    ] {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
}
