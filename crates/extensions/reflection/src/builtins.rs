//! Core attribute objects, for example `#[Attribute(Attribute::TARGET_PROPERTY)]`.

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassKind, ConstantValue, Context, MethodEntry, NativeResult, Object,
    Param, ParamType, PhpArray, PropertyEntry, PropertyType, Registry, Value, Visibility, args,
};
use std::rc::Rc;

use crate::{bind, load, receiver};

pub(crate) fn register(registry: &mut Registry) {
    use ConstantValue::Int;
    let builtin = |name| ClassEntry {
        kind: ClassKind::FinalClass,
        ..ClassEntry::class(name)
    };
    registry.class_in_module(
        "Core",
        ClassEntry {
            properties: const {
                &[PropertyEntry::typed(
                    "flags",
                    PropertyType::Int,
                    Visibility::Public,
                )]
            },
            constants: const {
                &[
                    ("TARGET_CLASS", Int(1)),
                    ("TARGET_FUNCTION", Int(2)),
                    ("TARGET_METHOD", Int(4)),
                    ("TARGET_PROPERTY", Int(8)),
                    ("TARGET_CLASS_CONSTANT", Int(16)),
                    ("TARGET_PARAMETER", Int(32)),
                    ("TARGET_CONSTANT", Int(64)),
                    ("TARGET_ALL", Int(127)),
                    ("IS_REPEATABLE", Int(128)),
                ]
            },
            methods: const {
                &[MethodEntry::public(
                    "__construct",
                    const { &[Param::new("flags", ParamType::Int).with_default(Int(127))] },
                    |_, object, args| {
                        write(
                            receiver(object)?,
                            "flags",
                            Value::Int(args::int(args, 0).unwrap_or(127)),
                        )
                    },
                )]
            },
            ..builtin("Attribute")
        },
    );
    for name in [
        "AllowDynamicProperties",
        "ReturnTypeWillChange",
        "Override",
        "SensitiveParameter",
    ] {
        registry.class_in_module(
            "Core",
            ClassEntry {
                methods: const {
                    &[MethodEntry::public("__construct", &[], |_, _, _| {
                        Ok(Value::Null)
                    })]
                },
                ..builtin(name)
            },
        );
    }
    registry.class_in_module("Core", builtin("DelayedTargetValidation"));
    registry.class_in_module(
        "Core",
        ClassEntry {
            properties: const {
                &[
                    MESSAGE,
                    PropertyEntry::typed("since", PropertyType::NullableString, Visibility::Public)
                        .readonly(),
                ]
            },
            methods: const {
                &[MethodEntry::public(
                    "__construct",
                    const {
                        &[
                            MESSAGE_ARG,
                            Param::new("since", ParamType::String)
                                .nullable()
                                .with_default(ConstantValue::Null),
                        ]
                    },
                    |_, object, args| {
                        let object = receiver(object)?;
                        write(object, "message", args.first().cloned().unwrap_or_default())?;
                        write(object, "since", args.get(1).cloned().unwrap_or_default())
                    },
                )]
            },
            ..builtin("Deprecated")
        },
    );
    registry.class_in_module(
        "Core",
        ClassEntry {
            properties: const { &[MESSAGE] },
            methods: const {
                &[MethodEntry::public(
                    "__construct",
                    const { &[MESSAGE_ARG] },
                    |_, object, args| {
                        write(
                            receiver(object)?,
                            "message",
                            args.first().cloned().unwrap_or_default(),
                        )
                    },
                )]
            },
            ..builtin("NoDiscard")
        },
    );
    registry.class_in_module(
        "Core",
        ClassEntry {
            methods: const {
                &[
                    MethodEntry::public(
                        "__construct",
                        const { &[Param::new("value", ParamType::Mixed)] },
                        |_, object, args| {
                            bind(
                                receiver(object)?,
                                None,
                                args.first().cloned().unwrap_or_default(),
                            )
                        },
                    ),
                    MethodEntry::public("getValue", &[], |_, object, _| load(object)),
                    MethodEntry::public("__debugInfo", &[], |_, _, _| {
                        Ok(Value::from(PhpArray::new()))
                    }),
                ]
            },
            hooks: Some(&SENSITIVE_HOOKS),
            ..builtin("SensitiveParameterValue")
        },
    );
}

const MESSAGE: PropertyEntry =
    PropertyEntry::typed("message", PropertyType::NullableString, Visibility::Public).readonly();
const MESSAGE_ARG: Param = Param::new("message", ParamType::String)
    .nullable()
    .with_default(ConstantValue::Null);

static SENSITIVE_HOOKS: phera_extension_api::ClassHooks = phera_extension_api::ClassHooks {
    properties: Some(sensitive_properties),
    ..phera_extension_api::ClassHooks::EMPTY
};

fn sensitive_properties(
    _: &mut dyn Context,
    _: &Object,
    _: phera_extension_api::PropertyPurpose,
) -> Result<PhpArray, phera_extension_api::NativeError> {
    Ok(PhpArray::new())
}

fn write(object: &Rc<Object>, name: &str, value: Value) -> NativeResult {
    object.set(ArrayKey::str(name), value);
    Ok(Value::Null)
}
