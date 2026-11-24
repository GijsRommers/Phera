//! `new ReflectionClassConstant(C::class, 'VERSION')` selects the declaring member.

use std::rc::Rc;

use phera_extension_api::reflection::{MemberInfo, MemberKind};
use phera_extension_api::{
    ArrayKey, ClassEntry, ConstantValue, Context, LocalFuture, MethodEntry, NativeResult, Object,
    Param, ParamType, PropertyEntry, PropertyType, Registry, Value, Visibility, args,
};

use crate::class::{NAME_PROPERTY, load_target_class, loaded_target_class, reflect_class};
use crate::{UNCLONEABLE, bind, engine, exception, inspect, load, receiver};

pub(crate) const CLASS_PROPERTY: PropertyEntry =
    PropertyEntry::typed("class", PropertyType::String, Visibility::Public);
pub(crate) const CLASS_ARGUMENT: Param = Param::new("class", ParamType::ObjectOrString);
pub(crate) const ACCESS: &[Param] = const { &[Param::new("accessible", ParamType::Bool)] };

pub(crate) fn register(registry: &mut Registry) {
    use ConstantValue::Int;
    registry.class(ClassEntry {
        interfaces: &["Reflector"],
        properties: const { &[NAME_PROPERTY, CLASS_PROPERTY] },
        constants: const {
            &[
                ("IS_PUBLIC", Int(1)),
                ("IS_PROTECTED", Int(2)),
                ("IS_PRIVATE", Int(4)),
                ("IS_FINAL", Int(32)),
            ]
        },
        methods: const {
            &[
                MethodEntry::public_async(
                    "__construct",
                    const { &[CLASS_ARGUMENT, Param::new("constant", ParamType::String)] },
                    |c, o, a| construct(c, o, a, MemberKind::Constant),
                )
                .with_ready(|c, o, a| construct_loaded(c, o, a, MemberKind::Constant)),
                MethodEntry::public("getName", &[], |_, o, _| {
                    data(o, |m| Value::from(m.name.as_str()))
                }),
                MethodEntry::public("getModifiers", &[], |_, o, _| {
                    data(o, |m| Value::Int(m.modifiers))
                }),
                MethodEntry::public("isPublic", &[], |_, o, _| has_modifier(o, 1)),
                MethodEntry::public("isProtected", &[], |_, o, _| has_modifier(o, 2)),
                MethodEntry::public("isPrivate", &[], |_, o, _| has_modifier(o, 4)),
                MethodEntry::public("isEnumCase", &[], |_, o, _| {
                    data(o, |m| Value::Bool(m.enum_case))
                })
                .returning("bool"),
                MethodEntry::public("isFinal", &[], |_, o, _| has_modifier(o, 32)),
                MethodEntry::public("getDocComment", &[], |_, o, _| doc(o)),
                MethodEntry::public("getValue", &[], |c, o, _| {
                    let member = reflected(o)?;
                    engine(c)?.reflect_class_constant(&member.class, &member.name)
                }),
                MethodEntry::public("hasType", &[], |_, o, _| {
                    data(o, |m| Value::Bool(m.declared_type.is_some()))
                }),
                MethodEntry::public("getType", &[], |c, o, _| {
                    crate::types::make(c, reflected(o)?.declared_type.as_deref())
                }),
                MethodEntry::public("isDeprecated", &[], |_, o, _| {
                    data(o, |m| {
                        Value::Bool(
                            m.attributes
                                .iter()
                                .any(|a| a.name.eq_ignore_ascii_case("Deprecated")),
                        )
                    })
                }),
                MethodEntry::public_async("getDeclaringClass", &[], declaring),
                MethodEntry::public_async("getAttributes", crate::attribute::FILTER, |c, o, a| {
                    attributes(c, o, a, "ReflectionClassConstant")
                }),
                MethodEntry::public("__toString", &[], |_, o, _| {
                    data(o, |m| {
                        Value::from(format!("Constant [ {}::{} ]\n", m.class, m.name))
                    })
                })
                .returning("string"),
            ]
        },
        hooks: Some(&UNCLONEABLE),
        ..ClassEntry::class("ReflectionClassConstant")
    });
    crate::property::register(registry);
}

pub(crate) fn construct(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
    kind: MemberKind,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let this = receiver(this.as_ref())?;
        let argument = args
            .first()
            .ok_or_else(|| exception("Missing reflection class name"))?;
        let info = load_target_class(context, argument).await?;
        bind_member(context, this, &args, kind, info)
    })
}

/// Bind an already loaded member, as in `new ReflectionProperty($post, 'title')`.
pub(crate) fn construct_loaded(
    context: &mut dyn Context,
    this: &Rc<Object>,
    args: &[Value],
    kind: MemberKind,
) -> Option<NativeResult> {
    let info = loaded_target_class(context, args.first()?)?;
    Some(bind_member(context, this, args, kind, info))
}

fn bind_member(
    context: &mut dyn Context,
    this: &Object,
    args: &[Value],
    kind: MemberKind,
    info: Rc<phera_extension_api::reflection::ClassInfo>,
) -> NativeResult {
    let object = match args.first() {
        Some(Value::Object(object)) => Some(Rc::downgrade(object)),
        _ => None,
    };
    let class = crate::class::ReflectedClass { info, object };
    let name = args::string(args, 1)
        .map(|name| String::from_utf8_lossy(name.as_bytes()))
        .unwrap_or_default();
    let member = crate::class_members::find(
        crate::class_members::members(context, &class, kind)?,
        &name,
        kind,
    )
    .ok_or_else(|| {
        let name = match kind {
            MemberKind::Method => format!("Method {}::{name}()", class.info.name),
            MemberKind::Property => format!("Property {}::${name}", class.info.name),
            MemberKind::Constant => format!("Constant {}::{name}", class.info.name),
        };
        exception(format!("{name} does not exist"))
    })?;
    this.set(ArrayKey::str("class"), Value::from(member.class.as_str()));
    bind(this, Some(&member.name), member.clone())
}

pub(crate) fn reflected(
    this: Option<&Rc<Object>>,
) -> Result<Rc<MemberInfo>, phera_extension_api::NativeError> {
    load(this)
}

pub(crate) fn data(
    this: Option<&Rc<Object>>,
    result: impl FnOnce(&MemberInfo) -> Value,
) -> NativeResult {
    inspect(this, |member: &Rc<MemberInfo>| result(member))
}

pub(crate) fn has_modifier(this: Option<&Rc<Object>>, mask: i64) -> NativeResult {
    data(this, |member| Value::Bool(member.modifiers & mask != 0))
}

pub(crate) fn doc(this: Option<&Rc<Object>>) -> NativeResult {
    data(this, |member| {
        member
            .source
            .as_ref()
            .and_then(|s| s.doc_comment.as_ref())
            .map_or(Value::Bool(false), |doc| Value::from(doc.clone()))
    })
}

pub(crate) fn declaring(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let member = reflected(this.as_ref())?;
        reflect_class(context, &member.class).await
    })
}

pub(crate) fn attributes<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
    class: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let member = reflected(this.as_ref())?;
        crate::attribute::list(context, &member.attributes, &args, class).await
    })
}

pub(crate) fn accessible(
    context: &mut dyn Context,
    class: &str,
) -> Result<phera_extension_api::MethodPreparation, phera_extension_api::NativeError> {
    phera_extension_api::deprecated_method(context, &format!("{class}::setAccessible"))
}
