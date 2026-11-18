//! `ReflectionConstant` exposes global names and values without fetching deprecated constants.

use std::rc::Rc;

use phera_extension_api::reflection::ConstantInfo;
use phera_extension_api::{
    ClassEntry, Context, MethodEntry, NativeResult, Object, Param, ParamType, Registry, Value, args,
};

use crate::class::{NAME_PROPERTY, split};
use crate::{UNCLONEABLE, bind, engine, load, receiver};

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["Reflector"],
        properties: &[NAME_PROPERTY],
        hooks: Some(&UNCLONEABLE),
        methods: const {
            &[
                MethodEntry::public(
                    "__construct",
                    const { &[Param::new("name", ParamType::String)] },
                    construct,
                ),
                MethodEntry::public("getName", &[], |_, o, _| {
                    read(o, |c| Value::from(c.name.as_str()))
                }),
                MethodEntry::public("getNamespaceName", &[], |_, o, _| {
                    read(o, |c| Value::from(split(&c.name).0))
                }),
                MethodEntry::public("getShortName", &[], |_, o, _| {
                    read(o, |c| Value::from(split(&c.name).1))
                }),
                MethodEntry::public("getValue", &[], |_, o, _| read(o, |c| c.value.clone())),
                MethodEntry::public("isDeprecated", &[], |_, o, _| {
                    read(o, |c| Value::Bool(c.deprecated))
                }),
                MethodEntry::public("getFileName", &[], |_, o, _| {
                    read(o, |c| {
                        c.source
                            .as_ref()
                            .map_or(Value::Bool(false), |s| Value::from(s.file.as_str()))
                    })
                }),
                MethodEntry::public("getExtensionName", &[], |_, o, _| {
                    read(o, |c| {
                        c.extension
                            .as_ref()
                            .map_or(Value::Bool(false), |s| Value::from(s.as_str()))
                    })
                }),
                MethodEntry::public("getExtension", &[], |c, o, _| {
                    match &constant(o)?.extension {
                        Some(name) => crate::extension::create(c, name),
                        None => Ok(Value::Null),
                    }
                }),
                MethodEntry::public_async("getAttributes", crate::attribute::FILTER, |c, o, a| {
                    Box::pin(async move {
                        let constant = constant(o.as_ref())?;
                        crate::attribute::list(c, &constant.attributes, &a, "ReflectionConstant")
                            .await
                    })
                }),
                MethodEntry::public("__toString", &[], |_, o, _| {
                    read(o, |c| Value::from(format!("Constant [ {} ]\n", c.name)))
                })
                .returning("string"),
            ]
        },
        ..ClassEntry::class("ReflectionConstant")
    });
}

fn construct(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let name = args::string(arguments, 0)
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let constant = engine(context)?.reflect_constant(&name)?;
    bind(receiver(this)?, Some(&name), Rc::new(constant))
}

fn constant(
    this: Option<&Rc<Object>>,
) -> Result<Rc<ConstantInfo>, phera_extension_api::NativeError> {
    load(this)
}

fn read(this: Option<&Rc<Object>>, value: impl FnOnce(&ConstantInfo) -> Value) -> NativeResult {
    Ok(value(&*constant(this)?))
}
