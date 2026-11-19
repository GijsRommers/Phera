//! `ReflectionProperty::setValue($object, $value)` bypasses ordinary property visibility.

use std::rc::Rc;

use phera_extension_api::reflection::{
    ClassSelector, MemberInfo, MemberKind, PropertyHookKind, PropertyRead, PropertyWrite,
};
use phera_extension_api::{
    ArrayKey, ClassEntry, ConstantValue, Context, MethodEntry, NativeError, NativeResult, Object,
    Param, ParamType, PhpArray, Registry, Value, args,
};

use crate::class::NAME_PROPERTY;
use crate::member::{
    ACCESS, CLASS_ARGUMENT, CLASS_PROPERTY, accessible, attributes, construct, data, declaring,
    doc, has_modifier, reflected,
};
use crate::{UNCLONEABLE, engine};

const OBJECT: &[Param] = const {
    &[Param::new("object", ParamType::Object)
        .nullable()
        .with_default(ConstantValue::Null)]
};
const HOOK: &[Param] = const { &[Param::new("type", ParamType::Class("PropertyHookType"))] };
const HOOKS: [PropertyHookKind; 2] = [PropertyHookKind::Get, PropertyHookKind::Set];
const RAW_OBJECT: &[Param] = const { &[Param::new("object", ParamType::Object)] };

pub(crate) fn register(registry: &mut Registry) {
    use ConstantValue::Int;
    registry.class(ClassEntry {
        interfaces: &["Reflector"],
        properties: const { &[NAME_PROPERTY, CLASS_PROPERTY] },
        constants: const {
            &[
                ("IS_STATIC", Int(16)),
                ("IS_READONLY", Int(128)),
                ("IS_PUBLIC", Int(1)),
                ("IS_PROTECTED", Int(2)),
                ("IS_PRIVATE", Int(4)),
                ("IS_ABSTRACT", Int(64)),
                ("IS_PROTECTED_SET", Int(2048)),
                ("IS_PRIVATE_SET", Int(4096)),
                ("IS_VIRTUAL", Int(512)),
                ("IS_FINAL", Int(32)),
            ]
        },
        methods: const {
            &[
                MethodEntry::public_async(
                    "__construct",
                    const { &[CLASS_ARGUMENT, Param::new("property", ParamType::String)] },
                    |c, o, a| construct(c, o, a, MemberKind::Property),
                ),
                MethodEntry::public("getName", &[], |_, o, _| {
                    data(o, |m| Value::from(m.name.as_str()))
                }),
                MethodEntry::public("getMangledName", &[], |_, o, _| {
                    data(o, |m| {
                        Value::from(match m.modifiers & 7 {
                            4 => format!("\0{}\0{}", m.class, m.name),
                            2 => format!("\0*\0{}", m.name),
                            _ => m.name.clone(),
                        })
                    })
                }),
                MethodEntry::public("getModifiers", &[], |_, o, _| {
                    data(o, |m| Value::Int(m.modifiers))
                }),
                MethodEntry::public("isPublic", &[], |_, o, _| has_modifier(o, 1)),
                MethodEntry::public("isProtected", &[], |_, o, _| has_modifier(o, 2)),
                MethodEntry::public("isPrivate", &[], |_, o, _| has_modifier(o, 4)),
                MethodEntry::public("isStatic", &[], |_, o, _| has_modifier(o, 16)),
                MethodEntry::public("isFinal", &[], |_, o, _| has_modifier(o, 32))
                    .returning("bool"),
                MethodEntry::public("isAbstract", &[], |_, o, _| has_modifier(o, 64))
                    .returning("bool"),
                MethodEntry::public("isReadOnly", &[], |_, o, _| has_modifier(o, 128)),
                MethodEntry::public("isVirtual", &[], |_, o, _| has_modifier(o, 512))
                    .returning("bool"),
                MethodEntry::public("isProtectedSet", &[], |_, o, _| has_modifier(o, 2048)),
                MethodEntry::public("isPrivateSet", &[], |_, o, _| has_modifier(o, 4096)),
                MethodEntry::public("isDefault", &[], |_, o, _| {
                    data(o, |m| Value::Bool(!m.dynamic))
                }),
                MethodEntry::public("isDynamic", &[], |_, o, _| {
                    data(o, |m| Value::Bool(m.dynamic))
                }),
                MethodEntry::public("isPromoted", &[], |_, o, _| {
                    data(o, |m| Value::Bool(m.promoted))
                }),
                MethodEntry::public("hasDefaultValue", &[], |_, o, _| {
                    data(o, |m| Value::Bool(m.has_default))
                }),
                MethodEntry::public("getDefaultValue", &[], default_value),
                MethodEntry::public("getValue", OBJECT, |c, o, a| {
                    read(c, o, a, PropertyRead::Value, "getValue")
                }),
                MethodEntry::public("isInitialized", OBJECT, |c, o, a| {
                    read(c, o, a, PropertyRead::Initialized, "isInitialized")
                }),
                MethodEntry::public("getRawValue", RAW_OBJECT, |c, o, a| {
                    read(c, o, a, PropertyRead::Raw, "getRawValue")
                })
                .returning("mixed"),
                MethodEntry::public(
                    "setRawValue",
                    const {
                        &[
                            Param::new("object", ParamType::Object),
                            Param::new("value", ParamType::Mixed),
                        ]
                    },
                    |c, o, a| {
                        let member = reflected(o)?;
                        if member.modifiers & 16 != 0 {
                            return Err(crate::exception(
                                "May not use setRawValue on static properties",
                            ));
                        }
                        set_raw(c, member, a)
                    },
                )
                .returning("void"),
                MethodEntry::public(
                    "setValue",
                    const {
                        &[
                            Param::new("objectOrValue", ParamType::Mixed).optional(),
                            Param::new("value", ParamType::Mixed).optional(),
                        ]
                    },
                    set_value,
                ),
                MethodEntry::public("hasType", &[], |_, o, _| {
                    data(o, |m| Value::Bool(m.declared_type.is_some()))
                }),
                MethodEntry::public("getType", &[], |c, o, _| {
                    crate::types::make(c, reflected(o)?.declared_type.as_deref())
                }),
                MethodEntry::public("getSettableType", &[], |c, o, _| {
                    let ty = engine(c)?.reflect_property_settable_type(&*reflected(o)?);
                    crate::types::make(c, ty.as_deref())
                })
                .returning("?ReflectionType"),
                MethodEntry::public("hasHooks", &[], |c, o, _| {
                    let member = reflected(o)?;
                    let engine = engine(c)?;
                    Ok(Value::Bool(HOOKS.iter().any(|kind| {
                        engine.reflect_property_hook(&member, *kind).is_some()
                    })))
                })
                .returning("bool"),
                MethodEntry::public("getHooks", &[], |c, o, _| {
                    let member = reflected(o)?;
                    let mut hooks = PhpArray::new();
                    for kind in HOOKS {
                        if let Some(target) = engine(c)?.reflect_property_hook(&member, kind) {
                            hooks.insert(
                                ArrayKey::str(kind.name()),
                                crate::function::make(c, target)?,
                            );
                        }
                    }
                    Ok(Value::from(hooks))
                })
                .returning("array"),
                MethodEntry::public("hasHook", HOOK, |c, o, a| {
                    let member = reflected(o)?;
                    let kind = hook_kind(a)?;
                    Ok(Value::Bool(
                        engine(c)?.reflect_property_hook(&member, kind).is_some(),
                    ))
                })
                .returning("bool"),
                MethodEntry::public("getHook", HOOK, |c, o, a| {
                    let member = reflected(o)?;
                    let kind = hook_kind(a)?;
                    match engine(c)?.reflect_property_hook(&member, kind) {
                        Some(target) => crate::function::make(c, target),
                        None => Ok(Value::Null),
                    }
                })
                .returning("?ReflectionMethod"),
                MethodEntry::public("getDocComment", &[], |_, o, _| doc(o)),
                MethodEntry::public("setAccessible", ACCESS, |_, o, _| {
                    reflected(o)?;
                    Ok(Value::Null)
                })
                .with_preparation(|c, _, _| accessible(c, "ReflectionProperty")),
                MethodEntry::public_async("getDeclaringClass", &[], declaring),
                MethodEntry::public_async("getAttributes", crate::attribute::FILTER, |c, o, a| {
                    attributes(c, o, a, "ReflectionProperty")
                }),
                MethodEntry::public("__toString", &[], |_, o, _| {
                    data(o, |m| Value::from(format!("Property [ ${} ]\n", m.name)))
                })
                .returning("string"),
            ]
        },
        hooks: Some(&UNCLONEABLE),
        ..ClassEntry::class("ReflectionProperty")
    });
}

fn read(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    args: &[Value],
    mode: PropertyRead,
    method: &str,
) -> NativeResult {
    let member = reflected(this)?;
    let object = args::object(args, 0).cloned();
    if method == "getRawValue" {
        if object
            .as_ref()
            .is_some_and(|object| !object.class.is_instance_of(&member.lookup_class))
        {
            return Err(crate::exception(
                "Given object is not an instance of the class this property was declared in",
            ));
        }
        if member.modifiers & 16 != 0 {
            return Err(crate::exception(
                "May not use getRawValue on static properties",
            ));
        }
    }
    if member.modifiers & 16 == 0 && object.is_none() {
        return Err(NativeError::type_error(format!(
            "ReflectionProperty::{method}(): Argument #1 ($object) must be provided for instance properties"
        )));
    }
    if member.dynamic
        && let Some(object) = object
    {
        if let Some(actual) = declared_property(context, &object, &member.name)? {
            if matches!(mode, PropertyRead::Raw) && actual.modifiers & 16 != 0 {
                return Err(crate::exception(
                    "May not use getRawValue on static properties",
                ));
            }
            return engine(context)?.reflect_property_read(&actual, Some(object), mode);
        }
        let value = object.get(&ArrayKey::str(member.name.as_str()));
        return if matches!(mode, PropertyRead::Initialized) {
            Ok(Value::Bool(value.is_some()))
        } else if let Some(value) = value {
            Ok(value.deref().clone())
        } else {
            let class = object
                .class
                .name()
                .split_once('\0')
                .map_or(object.class.name(), |(name, _)| name);
            context.warning(format!("Undefined property: {class}::${}", member.name));
            Ok(Value::Null)
        };
    }
    engine(context)?.reflect_property_read(&member, object, mode)
}

fn default_value(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    _: &[Value],
) -> NativeResult {
    let member = reflected(this)?;
    if !member.has_default {
        context.deprecated("ReflectionProperty::getDefaultValue() for a property without a default value is deprecated, use ReflectionProperty::hasDefaultValue() to check if the default value exists".into());
        return Ok(Value::Null);
    }
    engine(context)?.reflect_property_read(&member, None, PropertyRead::Default)
}

fn hook_kind(args: &[Value]) -> Result<PropertyHookKind, NativeError> {
    let name = args::object(args, 0).and_then(|object| object.get(&ArrayKey::str("name")));
    match name {
        Some(Value::String(name)) if name.as_bytes() == b"Get" => Ok(PropertyHookKind::Get),
        Some(Value::String(name)) if name.as_bytes() == b"Set" => Ok(PropertyHookKind::Set),
        _ => Err(NativeError::type_error("Expected a PropertyHookType case")),
    }
}

fn declared_property(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
) -> Result<Option<Rc<MemberInfo>>, NativeError> {
    Ok(engine(context)?
        .reflect_members(
            ClassSelector::Name(object.class.name()),
            MemberKind::Property,
        )
        .iter()
        .find(|candidate| candidate.name == name)
        .cloned())
}

fn set_raw(context: &mut dyn Context, member: Rc<MemberInfo>, args: &[Value]) -> NativeResult {
    let object = args::object(args, 0).cloned();
    let value = args.get(1).cloned().unwrap_or_default();
    if let Some(object) = object.as_ref()
        && (member.dynamic || !object.class.is_instance_of(&member.class))
        && let Some(actual) = declared_property(context, object, &member.name)?
    {
        if actual.modifiers & 16 != 0 {
            return Err(crate::exception(
                "May not use setRawValue on static properties",
            ));
        }
        return engine(context)?.reflect_property_write(
            &actual,
            Some(object.clone()),
            value,
            PropertyWrite::Raw,
        );
    }
    if member.dynamic
        && let Some(object) = object
    {
        return engine(context)?.reflect_dynamic_property_write(object, &member.name, value);
    }
    engine(context)?.reflect_property_write(&member, object, value, PropertyWrite::Raw)
}

fn set_value(context: &mut dyn Context, this: Option<&Rc<Object>>, args: &[Value]) -> NativeResult {
    let member = reflected(this)?;
    let (object, value) = write_arguments(context, &member, args)?;
    if member.dynamic
        && let Some(object) = object
    {
        engine(context)?.reflect_dynamic_property_write(object, &member.name, value)
    } else {
        engine(context)?.reflect_property_write(&member, object, value, PropertyWrite::Value)
    }
}

fn write_arguments(
    context: &mut dyn Context,
    member: &MemberInfo,
    args: &[Value],
) -> Result<(Option<Rc<Object>>, Value), NativeError> {
    let (object, value) = match args {
        [value] if member.modifiers & 16 != 0 => {
            context.deprecated(
                "Calling ReflectionProperty::setValue() with a single argument is deprecated"
                    .into(),
            );
            (None, value)
        }
        [_, value] if member.modifiers & 16 != 0 => (None, value),
        [Value::Object(object), value] => (Some(object.clone()), value),
        [first, _] => {
            return Err(NativeError::type_error(format!(
                "ReflectionProperty::setValue(): Argument #1 ($objectOrValue) must be of type object, {} given",
                first.type_name()
            )));
        }
        _ => {
            return Err(NativeError::throw(
                "ArgumentCountError",
                format!(
                    "ReflectionProperty::setValue() expects exactly 2 arguments, {} given",
                    args.len()
                ),
            ));
        }
    };
    Ok((object, value.clone()))
}
