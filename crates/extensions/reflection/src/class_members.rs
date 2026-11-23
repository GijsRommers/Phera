//! `ReflectionClass::getMethods(ReflectionMethod::IS_PUBLIC)` applies a union of modifiers.

use std::rc::Rc;

use phera_extension_api::reflection::{
    ClassSelector, MemberInfo, MemberKind, Members, PropertyRead,
};
use phera_extension_api::{
    ArrayKey, ConstantValue, Context, MethodEntry, NativeResult, Object, Param, ParamType,
    PhpArray, Value, args,
};

use crate::class::{ReflectedClass, reflected_class};
use crate::{allocate, engine, exception};

mod instance;

pub(crate) use instance::with_instance;
use instance::{append_instance_members, dynamic_property, properties};

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
    MethodEntry::public("getProperties", FILTER, properties),
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
) -> Result<Members, phera_extension_api::NativeError> {
    let mut result = engine(context)?.reflect_members(ClassSelector::Linked(class.info.id), kind);
    if matches!(kind, MemberKind::Property)
        && let Some(object) = class.object.as_ref().and_then(std::rc::Weak::upgrade)
    {
        append_instance_members(&mut result, &class.info.name, &object);
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
    if matches!(kind, MemberKind::Property) {
        let declared = engine(context)?.reflect_members(ClassSelector::Linked(class.info.id), kind);
        if find(declared, &name, kind).is_some() {
            return Ok(Value::Bool(true));
        }
        if let Some(object) = class.object.and_then(|object| object.upgrade()) {
            return engine(context)?.reflect_instance_has_property(object, name);
        }
        return Ok(Value::Bool(false));
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
    let declared = engine(context)?.reflect_members(ClassSelector::Linked(class.info.id), kind);
    if let Some(member) = find(declared, &name, kind) {
        return member_object(context, member, kind);
    }
    if matches!(kind, MemberKind::Property)
        && let Some(object) = class.object.and_then(|object| object.upgrade())
    {
        return with_instance(context, object, move |context, object| {
            if !name.starts_with('\0') && object.has(&ArrayKey::str(name.as_str())) {
                let member = dynamic_property(&class.info.name, name);
                return member_object(context, member, kind);
            }
            Err(exception(format!(
                "Property {}::${name} does not exist",
                class.info.name
            )))
        });
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

pub(crate) fn find(members: Members, name: &str, kind: MemberKind) -> Option<Rc<MemberInfo>> {
    members
        .iter()
        .find(|member| match kind {
            MemberKind::Method => member.name.eq_ignore_ascii_case(name),
            _ => member.name == name,
        })
        .cloned()
}

fn all(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    args: &[Value],
    kind: MemberKind,
) -> NativeResult {
    let class = reflected_class(this)?;
    let members = members(context, &class, kind)?;
    let mut result = PhpArray::with_capacity(members.len());
    for member in members.iter() {
        if matches_filter(member, args) {
            result.push(member_object(context, member.clone(), kind)?)?;
        }
    }
    Ok(result.into())
}

fn matches_filter(member: &MemberInfo, arguments: &[Value]) -> bool {
    args::int(arguments, 0).is_none_or(|filter| member.modifiers & filter != 0)
}

fn constants(context: &mut dyn Context, this: Option<&Rc<Object>>, args: &[Value]) -> NativeResult {
    let class = reflected_class(this)?;
    let mut result = PhpArray::new();
    for member in members(context, &class, MemberKind::Constant)?.iter() {
        if matches_filter(member, args) {
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
    let Some(_) = &class.info.constructor else {
        return Ok(Value::Null);
    };
    crate::function::make(
        context,
        phera_extension_api::reflection::FunctionTarget::Method {
            class: class.info.name.clone(),
            name: "__construct".into(),
        },
    )
}

pub(crate) fn member_object(
    context: &mut dyn Context,
    member: Rc<MemberInfo>,
    kind: MemberKind,
) -> NativeResult {
    if matches!(kind, MemberKind::Method) {
        // By its place in the class's member list, which the VM answers
        // without looking the method up by name.
        let target = match member.method {
            Some(handle) => phera_extension_api::reflection::FunctionTarget::Member(handle),
            None => phera_extension_api::reflection::FunctionTarget::Method {
                class: member.lookup_class.clone(),
                name: member.name.clone(),
            },
        };
        return crate::function::make(context, target);
    }
    let class = match kind {
        MemberKind::Method => "ReflectionMethod",
        MemberKind::Property => "ReflectionProperty",
        MemberKind::Constant => "ReflectionClassConstant",
    };
    let value = allocate(context, class, Some(&member.name), member.clone())?;
    if let Value::Object(object) = &value {
        object.set(ArrayKey::str("class"), Value::from(member.class.as_str()));
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
    Rc::make_mut(&mut declared).sort_by_key(|member| member.modifiers & 16 == 0);
    for member in declared.iter() {
        let is_static = member.modifiers & 16 != 0;
        if member.dynamic
            || matches!(mode, PropertyRead::Value) && !is_static
            || matches!(mode, PropertyRead::Default) && !member.has_default
        {
            continue;
        }
        if matches!(mode, PropertyRead::Value)
            && matches!(
                engine(context)?.reflect_property_read(member, None, PropertyRead::Initialized)?,
                Value::Bool(false)
            )
        {
            continue;
        }
        let value = engine(context)?.reflect_property_read(member, None, mode)?;
        result.insert(ArrayKey::str(member.name.as_str()), value);
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
    engine(context)?.reflect_property_write(
        &member,
        None,
        args.get(1).cloned().unwrap_or_default(),
        phera_extension_api::reflection::PropertyWrite::Value,
    )
}

fn static_member(
    context: &mut dyn Context,
    class: &ReflectedClass,
    name: &str,
) -> Result<Option<Rc<MemberInfo>>, phera_extension_api::NativeError> {
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
