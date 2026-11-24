//! Enum reflectors share the class and constant snapshots of their parent reflectors.

use std::{borrow::Cow, rc::Rc};

use phera_extension_api::reflection::{ClassInfo, MemberInfo, MemberKind, ReflectedClassKind};
use phera_extension_api::{
    ArrayKey, ClassEntry, Context, LocalFuture, MethodEntry, NativeError, NativeResult, Object,
    Param, ParamType, PhpArray, Registry, Value, args,
};

use crate::{
    UNCLONEABLE, allocate, bind, bind_value,
    class::{
        ReflectedClass, load_target_class, loaded_target_class, reflected_class, target_class,
    },
    class_members::{find, members},
    engine, exception, inspect, member, receiver,
};

const CASE_NAME: &[Param] = &[Param::new("name", ParamType::String)];
const CASE_ARGS: &[Param] = &[
    member::CLASS_ARGUMENT,
    Param::new("constant", ParamType::String),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        parent: Some("ReflectionClass"),
        methods: const {
            &[
                MethodEntry::public_async(
                    "__construct",
                    const { &[Param::new("objectOrClass", ParamType::ObjectOrString)] },
                    construct_enum,
                )
                .with_ready(construct_loaded_enum),
                MethodEntry::public("hasCase", CASE_NAME, has_case).returning("bool"),
                MethodEntry::public("getCase", CASE_NAME, get_case)
                    .returning("ReflectionEnumUnitCase"),
                MethodEntry::public("getCases", &[], get_cases).returning("array"),
                MethodEntry::public("isBacked", &[], |_, o, _| {
                    inspect(o, |class: &ReflectedClass| {
                        Value::Bool(class.info.enum_backing.is_some())
                    })
                })
                .returning("bool"),
                MethodEntry::public("getBackingType", &[], |c, o, _| {
                    let backing = inspect(o, |class: &ReflectedClass| class.info.enum_backing)?;
                    crate::types::make(c, backing)
                })
                .returning("?ReflectionNamedType"),
            ]
        },
        hooks: Some(&UNCLONEABLE),
        ..ClassEntry::class("ReflectionEnum")
    });
    registry.class(ClassEntry {
        parent: Some("ReflectionClassConstant"),
        methods: const {
            &[
                MethodEntry::public_async("__construct", CASE_ARGS, |c, o, a| {
                    construct_case(c, o, a, false)
                }),
                MethodEntry::public_async("getEnum", &[], get_enum).returning("ReflectionEnum"),
                MethodEntry::public("getValue", &[], case_value).returning("UnitEnum"),
            ]
        },
        hooks: Some(&UNCLONEABLE),
        ..ClassEntry::class("ReflectionEnumUnitCase")
    });
    registry.class(ClassEntry {
        parent: Some("ReflectionEnumUnitCase"),
        methods: const {
            &[
                MethodEntry::public_async("__construct", CASE_ARGS, |c, o, a| {
                    construct_case(c, o, a, true)
                }),
                MethodEntry::public("getBackingValue", &[], backing_value).returning("string|int"),
            ]
        },
        hooks: Some(&UNCLONEABLE),
        ..ClassEntry::class("ReflectionEnumBackedCase")
    });
}

fn construct_enum(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let target = args
            .first()
            .ok_or_else(|| exception("Missing reflection class name"))?;
        let info = load_target_class(context, target).await?;
        bind_enum(receiver(this.as_ref())?, info)
    })
}

/// Bind `new ReflectionEnum(Status::class)` once Status is linked.
fn construct_loaded_enum(
    context: &mut dyn Context,
    this: &Rc<Object>,
    args: &[Value],
) -> Option<NativeResult> {
    let info = loaded_target_class(context, args.first()?)?;
    Some(bind_enum(this, info))
}

fn bind_enum(this: &Object, info: Rc<ClassInfo>) -> NativeResult {
    if info.kind != ReflectedClassKind::Enum {
        return Err(exception(format!("Class \"{}\" is not an enum", info.name)));
    }
    bind_value(
        this,
        Some(info.name_value.clone()),
        ReflectedClass { info, object: None },
    )
}

fn construct_case(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
    backed: bool,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let class = reflect_target(&args)?;
        let info = engine(context)?.reflect_class(&class).await?;
        let class = ReflectedClass { info, object: None };
        let name = case_name(&args, 1);
        let member = lookup(context, &class, &name)?.ok_or_else(|| {
            exception(format!(
                "Constant {}::{name} does not exist",
                class.info.name
            ))
        })?;
        if !member.enum_case {
            return Err(exception(format!(
                "Constant {}::{name} is not a case",
                class.info.name
            )));
        }
        if backed && class.info.enum_backing.is_none() {
            return Err(exception(format!(
                "Enum case {}::{name} is not a backed case",
                class.info.name
            )));
        }
        let this = receiver(this.as_ref())?;
        this.set(ArrayKey::str("class"), Value::from(member.class.as_str()));
        bind(this, Some(&name), member)
    })
}

fn reflect_target(args: &[Value]) -> Result<Cow<'_, str>, NativeError> {
    target_class(
        args.first()
            .ok_or_else(|| exception("Missing reflection class name"))?,
    )
}

fn case_name(args: &[Value], index: usize) -> String {
    args::string(args, index)
        .map(|name| name.to_string_lossy())
        .unwrap_or_default()
}

fn lookup(
    context: &mut dyn Context,
    class: &ReflectedClass,
    name: &str,
) -> Result<Option<Rc<MemberInfo>>, NativeError> {
    Ok(find(
        members(context, class, MemberKind::Constant)?,
        name,
        MemberKind::Constant,
    ))
}

fn has_case(context: &mut dyn Context, this: Option<&Rc<Object>>, args: &[Value]) -> NativeResult {
    let class = reflected_class(this)?;
    let member = lookup(context, &class, &case_name(args, 0))?;
    Ok(Value::Bool(member.is_some_and(|member| member.enum_case)))
}

fn get_case(context: &mut dyn Context, this: Option<&Rc<Object>>, args: &[Value]) -> NativeResult {
    let class = reflected_class(this)?;
    let name = case_name(args, 0);
    let member = lookup(context, &class, &name)?
        .ok_or_else(|| exception(format!("Case {}::{name} does not exist", class.info.name)))?;
    if !member.enum_case {
        return Err(exception(format!(
            "{}::{name} is not a case",
            class.info.name
        )));
    }
    allocate_case(context, &class, member)
}

fn get_cases(context: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let class = reflected_class(this)?;
    let constants = members(context, &class, MemberKind::Constant)?;
    let mut cases = PhpArray::new();
    for member in constants.iter().filter(|member| member.enum_case) {
        cases.push(allocate_case(context, &class, member.clone())?)?;
    }
    Ok(Value::from(cases))
}

fn allocate_case(
    context: &mut dyn Context,
    class: &ReflectedClass,
    member: Rc<MemberInfo>,
) -> NativeResult {
    let reflector = if class.info.enum_backing.is_some() {
        "ReflectionEnumBackedCase"
    } else {
        "ReflectionEnumUnitCase"
    };
    let value = allocate(context, reflector, Some(&member.name), member.clone())?;
    if let Value::Object(object) = &value {
        object.set(ArrayKey::str("class"), Value::from(member.class.as_str()));
    }
    Ok(value)
}

fn get_enum(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let case = member::reflected(this.as_ref())?;
        let info = engine(context)?.reflect_class(&case.class).await?;
        allocate(
            context,
            "ReflectionEnum",
            Some(&info.name),
            ReflectedClass {
                info: info.clone(),
                object: None,
            },
        )
    })
}

fn case_value(context: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let case = member::reflected(this)?;
    engine(context)?.reflect_class_constant(&case.class, &case.name)
}

fn backing_value(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    args: &[Value],
) -> NativeResult {
    if let Value::Object(case) = case_value(context, this, args)?
        && let Some(value) = case.get(&ArrayKey::str("value"))
    {
        return Ok(value);
    }
    Err(exception("Enum case has no backing value"))
}
