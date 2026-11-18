//! `ReflectionClass::getMethods(ReflectionMethod::IS_PUBLIC)` applies a union of modifiers.

use std::rc::Rc;

use phera_extension_api::reflection::{MemberInfo, MemberKind, PropertyRead};
use phera_extension_api::{
    ArrayKey, ConstantValue, Context, MethodEntry, NativeResult, Object, Param, ParamType,
    PhpArray, Value, args,
};

use crate::class::{ReflectedClass, reflected_class};
use crate::{allocate, engine, exception, values};

const NAME: &[Param] = const { &[Param::new("name", ParamType::String)] };
const FILTER: &[Param] = const {
    &[Param::new("filter", ParamType::Int)
        .nullable()
        .with_default(ConstantValue::Null)]
};

pub(crate) const METHODS: [MethodEntry; 17] = [
    MethodEntry::public("hasMethod", NAME, |c, o, a| {
        has(c, o, a, MemberKind::Method)
    }),
    MethodEntry::public("hasProperty", NAME, |c, o, a| {
        has(c, o, a, MemberKind::Property)
    }),
    MethodEntry::public("hasConstant", NAME, |c, o, a| {
        has(c, o, a, MemberKind::Constant)
    }),
    MethodEntry::public("getMethod", NAME, |c, o, a| {
        one(c, o, a, MemberKind::Method)
    }),
    MethodEntry::public("getProperty", NAME, |c, o, a| {
        one(c, o, a, MemberKind::Property)
    }),
    MethodEntry::public("getReflectionConstant", NAME, |c, o, a| {
        one(c, o, a, MemberKind::Constant)
    }),
    MethodEntry::public("getMethods", FILTER, |c, o, a| {
        all(c, o, a, MemberKind::Method)
    }),
    MethodEntry::public("getProperties", FILTER, |c, o, a| {
        all(c, o, a, MemberKind::Property)
    }),
    MethodEntry::public("getReflectionConstants", FILTER, |c, o, a| {
        all(c, o, a, MemberKind::Constant)
    }),
    MethodEntry::public("getConstants", FILTER, constants),
    MethodEntry::public("getConstant", NAME, constant),
    MethodEntry::public("getConstructor", &[], constructor),
    MethodEntry::public("getStaticProperties", &[], |c, o, _| {
        property_values(c, o, PropertyRead::Value)
    }),
    MethodEntry::public("getDefaultProperties", &[], |c, o, _| {
        property_values(c, o, PropertyRead::Default)
    }),
    MethodEntry::public(
        "getStaticPropertyValue",
        const {
            &[
                Param::new("name", ParamType::String),
                Param::new("default", ParamType::Mixed).optional(),
            ]
        },
        static_value,
    ),
    MethodEntry::public(
        "setStaticPropertyValue",
        const {
            &[
                Param::new("name", ParamType::String),
                Param::new("value", ParamType::Mixed),
            ]
        },
        set_static_value,
    ),
    MethodEntry::public("getExtension", &[], |c, o, _| {
        let class = reflected_class(o)?;
        match &class.info.extension {
            Some(name) => crate::extension::create(c, name),
            None => Ok(Value::Null),
        }
    }),
];

pub(crate) fn members(
    context: &mut dyn Context,
    class: &ReflectedClass,
    kind: MemberKind,
) -> Result<Vec<MemberInfo>, phera_extension_api::NativeError> {
    let mut result = engine(context)?.reflect_members(&class.info.name, kind);
    if matches!(kind, MemberKind::Property)
        && let Some(object) = &class.object
    {
        for (key, _) in object.to_array().iter() {
            let ArrayKey::String(key) = key else { continue };
            if key.as_bytes().starts_with(b"\0") {
                continue;
            }
            let name = key.to_string_lossy();
            if !result.iter().any(|property| property.name == name) {
                result.push(MemberInfo {
                    name,
                    class: class.info.name.clone(),
                    lookup_class: class.info.name.clone(),
                    modifiers: 1,
                    dynamic: true,
                    ..MemberInfo::default()
                });
            }
        }
    }
    Ok(result)
}

fn has(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    args: &[Value],
    kind: MemberKind,
) -> NativeResult {
    let name = argument_name(args);
    let class = reflected_class(this)?;
    if matches!(kind, MemberKind::Method) {
        let target = phera_extension_api::reflection::FunctionTarget::Method {
            class: class.info.name.clone(),
            name,
        };
        return Ok(Value::Bool(
            engine(context)?.reflect_function(&target).is_ok(),
        ));
    }
    Ok(Value::Bool(
        find(members(context, &class, kind)?, &name, kind).is_some(),
    ))
}

fn one(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    args: &[Value],
    kind: MemberKind,
) -> NativeResult {
    let name = argument_name(args);
    let class = reflected_class(this)?;
    if matches!(kind, MemberKind::Method) {
        return crate::function::make(
            context,
            phera_extension_api::reflection::FunctionTarget::Method {
                class: class.info.name.clone(),
                name,
            },
        );
    }
    if let Some(member) = find(members(context, &class, kind)?, &name, kind) {
        return member_object(context, member, kind);
    }
    match kind {
        MemberKind::Constant => Ok(Value::Bool(false)),
        MemberKind::Method => Err(exception(format!(
            "Method {}::{name}() does not exist",
            class.info.name
        ))),
        MemberKind::Property => Err(exception(format!(
            "Property {}::${name} does not exist",
            class.info.name
        ))),
    }
}

pub(crate) fn find(members: Vec<MemberInfo>, name: &str, kind: MemberKind) -> Option<MemberInfo> {
    members.into_iter().find(|member| match kind {
        MemberKind::Method => member.name.eq_ignore_ascii_case(name),
        _ => member.name == name,
    })
}

fn all(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    args: &[Value],
    kind: MemberKind,
) -> NativeResult {
    let class = reflected_class(this)?;
    let mut result = Vec::new();
    for member in members(context, &class, kind)? {
        if matches_filter(&member, args) {
            result.push(member_object(context, member, kind)?);
        }
    }
    values(result)
}

fn matches_filter(member: &MemberInfo, arguments: &[Value]) -> bool {
    args::int(arguments, 0).is_none_or(|filter| member.modifiers & filter != 0)
}

fn constants(context: &mut dyn Context, this: Option<&Rc<Object>>, args: &[Value]) -> NativeResult {
    let class = reflected_class(this)?;
    let mut result = PhpArray::new();
    for member in members(context, &class, MemberKind::Constant)? {
        if matches_filter(&member, args) {
            let value = engine(context)?.reflect_class_constant(&member.class, &member.name)?;
            result.insert(ArrayKey::str(member.name.as_str()), value);
        }
    }
    Ok(Value::from(result))
}

fn constant(context: &mut dyn Context, this: Option<&Rc<Object>>, args: &[Value]) -> NativeResult {
    let name = argument_name(args);
    let class = reflected_class(this)?;
    let member = find(
        members(context, &class, MemberKind::Constant)?,
        &name,
        MemberKind::Constant,
    );
    match member {
        Some(member) => engine(context)?.reflect_class_constant(&member.class, &name),
        None => {
            context.deprecated(
                "ReflectionClass::getConstant() for a non-existent constant is deprecated, use ReflectionClass::hasConstant() to check if the constant exists".into(),
            );
            Ok(Value::Bool(false))
        }
    }
}

fn constructor(context: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let class = reflected_class(this)?;
    let Some(owner) = &class.info.constructor else {
        return Ok(Value::Null);
    };
    let mut member = engine(context)?
        .reflect_members(owner, MemberKind::Method)
        .into_iter()
        .find(|method| method.name.eq_ignore_ascii_case("__construct"))
        .ok_or_else(|| exception("Constructor does not exist"))?;
    member.lookup_class = class.info.name.clone();
    member_object(context, member, MemberKind::Method)
}

pub(crate) fn member_object(
    context: &mut dyn Context,
    member: MemberInfo,
    kind: MemberKind,
) -> NativeResult {
    if matches!(kind, MemberKind::Method) {
        return crate::function::make(
            context,
            phera_extension_api::reflection::FunctionTarget::Method {
                class: member.lookup_class,
                name: member.name,
            },
        );
    }
    let class = match kind {
        MemberKind::Method => "ReflectionMethod",
        MemberKind::Property => "ReflectionProperty",
        MemberKind::Constant => "ReflectionClassConstant",
    };
    let name = member.name.clone();
    let owner = member.class.clone();
    let value = allocate(context, class, Some(&name), Rc::new(member))?;
    if let Value::Object(object) = &value {
        object.set(ArrayKey::str("class"), Value::from(owner));
    }
    Ok(value)
}

fn property_values(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    mode: PropertyRead,
) -> NativeResult {
    let class = reflected_class(this)?;
    let mut result = PhpArray::new();
    let mut declared = members(context, &class, MemberKind::Property)?;
    declared.sort_by_key(|member| member.modifiers & 16 == 0);
    for member in declared {
        let is_static = member.modifiers & 16 != 0;
        if member.dynamic
            || matches!(mode, PropertyRead::Value) && !is_static
            || matches!(mode, PropertyRead::Default) && !member.has_default
        {
            continue;
        }
        if matches!(mode, PropertyRead::Value)
            && matches!(
                engine(context)?.reflect_property_read(&member, None, PropertyRead::Initialized)?,
                Value::Bool(false)
            )
        {
            continue;
        }
        let value = engine(context)?.reflect_property_read(&member, None, mode)?;
        result.insert(ArrayKey::str(member.name), value);
    }
    Ok(Value::from(result))
}

fn static_value(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    args: &[Value],
) -> NativeResult {
    let name = argument_name(args);
    let class = reflected_class(this)?;
    match static_member(context, &class, &name)? {
        Some(member) => engine(context)?.reflect_property_read(&member, None, PropertyRead::Value),
        None => args.get(1).cloned().ok_or_else(|| {
            exception(format!(
                "Property {}::${name} does not exist",
                class.info.name
            ))
        }),
    }
}

fn set_static_value(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    args: &[Value],
) -> NativeResult {
    let name = argument_name(args);
    let class = reflected_class(this)?;
    let member = static_member(context, &class, &name)?.ok_or_else(|| {
        exception(format!(
            "Class {} does not have a property named {name}",
            class.info.name
        ))
    })?;
    engine(context)?.reflect_property_write(&member, None, args.get(1).cloned().unwrap_or_default())
}

fn static_member(
    context: &mut dyn Context,
    class: &ReflectedClass,
    name: &str,
) -> Result<Option<MemberInfo>, phera_extension_api::NativeError> {
    Ok(find(
        members(context, class, MemberKind::Property)?,
        name,
        MemberKind::Property,
    )
    .filter(|member| member.modifiers & 16 != 0))
}

fn argument_name(args: &[Value]) -> String {
    args::string(args, 0)
        .map(|name| name.to_string_lossy())
        .unwrap_or_default()
}
