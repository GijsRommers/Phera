//! `ReflectionExtension('Reflection')` reports the classes registered to PHP's Reflection module.

use std::rc::Rc;

use phera_extension_api::reflection::ExtensionInfo;
use phera_extension_api::{
    ArrayKey, ClassEntry, Context, LocalFuture, MethodEntry, NativeResult, Object, Param,
    ParamType, PhpArray, Registry, Value, args,
};

use crate::{UNCLONEABLE, allocate, bind, engine, exception, load, receiver, values};

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["Reflector"],
        properties: &[crate::class::NAME_PROPERTY],
        methods: const {
            &[
                MethodEntry::public(
                    "__construct",
                    const { &[Param::new("name", ParamType::String)] },
                    construct,
                ),
                MethodEntry::public("getName", &[], |_, o, _| {
                    Ok(Value::from(extension(o)?.name.as_str()))
                }),
                MethodEntry::public("getVersion", &[], |_, o, _| {
                    Ok(Value::from(extension(o)?.version.as_str()))
                }),
                MethodEntry::public("getClassNames", &[], |_, o, _| {
                    values(
                        extension(o)?
                            .classes
                            .iter()
                            .map(|name| Value::from(name.as_str())),
                    )
                }),
                MethodEntry::public("getConstants", &[], |_, o, _| {
                    Ok(Value::from(extension(o)?.constants.clone()))
                }),
                MethodEntry::public("isPersistent", &[], |_, o, _| {
                    extension(o)?;
                    Ok(Value::Bool(true))
                }),
                MethodEntry::public("isTemporary", &[], |_, o, _| {
                    extension(o)?;
                    Ok(Value::Bool(false))
                }),
                MethodEntry::public_async("getClasses", &[], classes),
                MethodEntry::public("getFunctions", &[], functions),
                MethodEntry::public("__toString", &[], |_, o, _| {
                    Ok(Value::from(format!(
                        "Extension [ {} ]\n",
                        extension(o)?.name
                    )))
                })
                .returning("string"),
            ]
        },
        hooks: Some(&UNCLONEABLE),
        ..ClassEntry::class("ReflectionExtension")
    });
}

fn extension(
    this: Option<&Rc<Object>>,
) -> Result<Rc<ExtensionInfo>, phera_extension_api::NativeError> {
    load(this)
}

fn info(
    context: &mut dyn Context,
    name: &str,
) -> Result<Rc<ExtensionInfo>, phera_extension_api::NativeError> {
    engine(context)?
        .reflect_extension(name)
        .map(Rc::new)
        .ok_or_else(|| exception(format!("Extension \"{name}\" does not exist")))
}

pub(crate) fn create(context: &mut dyn Context, name: &str) -> NativeResult {
    let info = info(context, name)?;
    allocate(context, "ReflectionExtension", Some(name), info)
}

fn construct(context: &mut dyn Context, this: Option<&Rc<Object>>, args: &[Value]) -> NativeResult {
    let name = args::string(args, 0)
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let info = info(context, &name)?;
    bind(receiver(this)?, Some(&info.name.clone()), info)
}

fn classes(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let mut result = PhpArray::new();
        for name in &extension(this.as_ref())?.classes {
            result.insert(
                ArrayKey::str(name.as_str()),
                crate::class::reflect_class(context, name).await?,
            );
        }
        Ok(Value::from(result))
    })
}

fn functions(context: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let extension = extension(this)?;
    let mut result = PhpArray::new();
    for name in &extension.functions {
        let target = phera_extension_api::reflection::FunctionTarget::Named(name.clone());
        result.insert(
            ArrayKey::str(name.as_str()),
            crate::function::make(context, target)?,
        );
    }
    Ok(Value::from(result))
}
