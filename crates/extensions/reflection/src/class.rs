//! `ReflectionClass` exposes class identity, source locations and construction.

use std::rc::{Rc, Weak};

use phera_extension_api::reflection::{ClassInfo, ReflectedClassKind};
use phera_extension_api::{
    ArrayKey, ClassEntry, ConstantValue, Context, LocalFuture, MethodEntry, NativeResult, Object,
    Param, ParamType, PhpArray, PropertyEntry, PropertyType, Registry, Value, Visibility, args,
};

use crate::{UNCLONEABLE, allocate, bind, engine, exception, load, receiver, values};

pub(crate) const NAME_PROPERTY: PropertyEntry =
    PropertyEntry::typed("name", PropertyType::String, Visibility::Public);

#[derive(Clone)]
pub(crate) struct ReflectedClass {
    pub info: Rc<ClassInfo>,
    pub object: Option<Weak<Object>>,
}

pub(crate) fn register(registry: &mut Registry) {
    use ConstantValue::Int;
    registry.class(ClassEntry {
        interfaces: &["Stringable", "Reflector"],
        properties: const { &[NAME_PROPERTY] },
        constants: const {
            &[
                ("IS_IMPLICIT_ABSTRACT", Int(16)),
                ("IS_EXPLICIT_ABSTRACT", Int(64)),
                ("IS_FINAL", Int(32)),
                ("IS_READONLY", Int(65536)),
                ("SKIP_INITIALIZATION_ON_SERIALIZE", Int(8)),
                ("SKIP_DESTRUCTOR", Int(16)),
            ]
        },
        methods: &METHODS,
        hooks: Some(&UNCLONEABLE),
        ..ClassEntry::class("ReflectionClass")
    });
    registry.class(ClassEntry {
        parent: Some("ReflectionClass"),
        methods: const {
            &[MethodEntry::public_async(
                "__construct",
                const { &[Param::new("object", ParamType::Object)] },
                construct,
            )]
        },
        ..ClassEntry::class("ReflectionObject")
    });
}

const IDENTITY_METHODS: &[MethodEntry] = const {
    &[
        MethodEntry::public_async(
            "__construct",
            const { &[Param::new("objectOrClass", ParamType::ObjectOrString)] },
            construct,
        ),
        MethodEntry::public("getName", &[], |_, o, _| {
            data(o, |r| Value::from(r.name.as_str()))
        }),
        MethodEntry::public("getShortName", &[], |_, o, _| {
            data(o, |r| Value::from(split(&r.name).1))
        }),
        MethodEntry::public("getNamespaceName", &[], |_, o, _| {
            data(o, |r| Value::from(split(&r.name).0))
        }),
        MethodEntry::public("inNamespace", &[], |_, o, _| {
            data(o, |r| Value::Bool(!split(&r.name).0.is_empty()))
        }),
        MethodEntry::public("isInternal", &[], |_, o, _| {
            data(o, |r| Value::Bool(r.source.is_none()))
        }),
        MethodEntry::public("isUserDefined", &[], |_, o, _| {
            data(o, |r| Value::Bool(r.source.is_some()))
        }),
        MethodEntry::public("isAnonymous", &[], |_, o, _| {
            data(o, |r| Value::Bool(r.anonymous))
        }),
        MethodEntry::public("isInstantiable", &[], |_, o, _| {
            data(o, |r| Value::Bool(r.instantiable))
        }),
        MethodEntry::public("isCloneable", &[], |_, o, _| {
            data(o, |r| Value::Bool(r.cloneable))
        }),
        MethodEntry::public("isIterable", &[], |_, o, _| {
            data(o, |r| Value::Bool(r.iterable))
        }),
        MethodEntry::public("isIterateable", &[], |_, o, _| {
            data(o, |r| Value::Bool(r.iterable))
        }),
        MethodEntry::public("isAbstract", &[], |_, o, _| {
            data(o, |r| Value::Bool(r.is_abstract))
        }),
        MethodEntry::public("isFinal", &[], |_, o, _| {
            data(o, |r| Value::Bool(r.modifiers & 32 != 0))
        }),
        MethodEntry::public("isReadOnly", &[], |_, o, _| {
            data(o, |r| Value::Bool(r.modifiers & 65536 != 0))
        }),
        MethodEntry::public("isInterface", &[], |_, o, _| {
            data(o, |r| Value::Bool(r.kind == ReflectedClassKind::Interface))
        }),
        MethodEntry::public("isTrait", &[], |_, o, _| {
            data(o, |r| Value::Bool(r.kind == ReflectedClassKind::Trait))
        }),
        MethodEntry::public("isEnum", &[], |_, o, _| {
            data(o, |r| Value::Bool(r.kind == ReflectedClassKind::Enum))
        }),
        MethodEntry::public("getModifiers", &[], |_, o, _| {
            data(o, |r| Value::Int(r.modifiers))
        }),
        MethodEntry::public("getFileName", &[], |_, o, _| {
            data(o, |r| {
                r.source
                    .as_ref()
                    .map_or(Value::Bool(false), |s| Value::from(s.file.as_str()))
            })
        }),
        MethodEntry::public("getStartLine", &[], |_, o, _| {
            data(o, |r| {
                r.source
                    .as_ref()
                    .map_or(Value::Bool(false), |s| Value::Int(s.start_line.into()))
            })
        }),
        MethodEntry::public("getEndLine", &[], |_, o, _| {
            data(o, |r| {
                r.source
                    .as_ref()
                    .map_or(Value::Bool(false), |s| Value::Int(s.end_line.into()))
            })
        }),
        MethodEntry::public("getDocComment", &[], |_, o, _| {
            data(o, |r| {
                r.source
                    .as_ref()
                    .and_then(|s| s.doc_comment.as_ref())
                    .map_or(Value::Bool(false), |doc| Value::from(doc.clone()))
            })
        }),
        MethodEntry::public("getExtensionName", &[], |_, o, _| {
            data(o, |r| {
                r.extension
                    .as_ref()
                    .map_or(Value::Bool(false), |name| Value::from(name.as_str()))
            })
        }),
        MethodEntry::public(
            "isInstance",
            const { &[Param::new("object", ParamType::Object)] },
            |_, o, a| {
                data(o, |r| {
                    Value::Bool(
                        args::object(a, 0)
                            .is_some_and(|object| object.class.is_instance_of(&r.name)),
                    )
                })
            },
        ),
        MethodEntry::public_async("getParentClass", &[], parent),
        MethodEntry::public("getInterfaceNames", &[], |_, o, _| {
            names(o, |r| &r.interfaces)
        }),
        MethodEntry::public("getTraitNames", &[], |_, o, _| names(o, |r| &r.traits)),
        MethodEntry::public("getTraitAliases", &[], aliases),
        MethodEntry::public_async("getInterfaces", &[], |c, o, _| {
            relations(c, o, |r| &r.interfaces)
        }),
        MethodEntry::public_async("getTraits", &[], |c, o, _| relations(c, o, |r| &r.traits)),
        MethodEntry::public_async(
            "isSubclassOf",
            const { &[Param::new("class", ParamType::ObjectOrString)] },
            |c, o, a| relationship(c, o, a, Relationship::Subclass),
        ),
        MethodEntry::public_async(
            "implementsInterface",
            const { &[Param::new("interface", ParamType::ObjectOrString)] },
            |c, o, a| relationship(c, o, a, Relationship::Interface),
        ),
        MethodEntry::public_async_named(
            "newInstance",
            const { &[Param::new("args", ParamType::Mixed).variadic()] },
            |c, o, a, names| {
                Box::pin(async move {
                    let arguments = crate::function::forwarded_arguments(a, 0, names)?;
                    instantiate(c, o, Some(arguments)).await
                })
            },
        ),
        MethodEntry::public_async(
            "newInstanceArgs",
            const { &[Param::new("args", ParamType::Array).optional()] },
            |c, o, a| {
                let arguments = args::array(&a, 0)
                    .map(|a| (**a).clone())
                    .unwrap_or_default();
                instantiate(c, o, Some(arguments))
            },
        ),
        MethodEntry::public_async("newInstanceWithoutConstructor", &[], |c, o, _| {
            instantiate(c, o, None)
        }),
        MethodEntry::public_async("getAttributes", crate::attribute::FILTER, |c, o, a| {
            Box::pin(async move {
                let target = reflected_class(o.as_ref())?;
                crate::attribute::class_attributes(c, &target.info, &a).await
            })
        }),
        MethodEntry::public("__toString", &[], |_, o, _| {
            data(o, |r| {
                Value::from(format!(
                    "{} [ {} class {} ]\n",
                    if r.source.is_some() {
                        "Class"
                    } else {
                        "Class <internal>"
                    },
                    "",
                    r.name
                ))
            })
        })
        .returning("string"),
    ]
};

const METHODS: [MethodEntry; IDENTITY_METHODS.len() + crate::class_members::METHODS.len()] = {
    let mut methods =
        [IDENTITY_METHODS[0]; IDENTITY_METHODS.len() + crate::class_members::METHODS.len()];
    let mut index = 0;
    while index < IDENTITY_METHODS.len() {
        methods[index] = IDENTITY_METHODS[index];
        index += 1;
    }
    let mut member = 0;
    while member < crate::class_members::METHODS.len() {
        methods[index] = crate::class_members::METHODS[member];
        index += 1;
        member += 1;
    }
    methods
};

fn construct(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let this = receiver(this.as_ref())?;
        let argument = args
            .first()
            .ok_or_else(|| exception("Missing reflection class name"))?;
        let name = target_class(argument)?;
        let info = engine(context)?.reflect_class(&name).await?;
        let object = if this.class.is_instance_of("ReflectionObject") {
            match argument {
                Value::Object(object) => {
                    crate::keep(this, Value::Object(object.clone()));
                    Some(Rc::downgrade(object))
                }
                _ => None,
            }
        } else {
            None
        };
        bind(
            this,
            Some(&info.name),
            ReflectedClass {
                info: info.clone(),
                object,
            },
        )
    })
}

pub(crate) fn target_class(value: &Value) -> Result<String, phera_extension_api::NativeError> {
    match value {
        Value::Object(object) => Ok(object.class.name().to_owned()),
        Value::String(name) => Ok(name.to_string_lossy()),
        _ => Err(exception("Expected a class name or object")),
    }
}

pub(crate) fn reflected_class(
    this: Option<&Rc<Object>>,
) -> Result<ReflectedClass, phera_extension_api::NativeError> {
    load(this)
}

pub(crate) async fn reflect_class(context: &mut dyn Context, name: &str) -> NativeResult {
    let info = engine(context)?.reflect_class(name).await?;
    allocate(
        context,
        "ReflectionClass",
        Some(&info.name),
        ReflectedClass {
            info: info.clone(),
            object: None,
        },
    )
}

pub(crate) fn split(name: &str) -> (&str, &str) {
    name.rsplit_once('\\').unwrap_or(("", name))
}

fn data(this: Option<&Rc<Object>>, result: impl FnOnce(&ClassInfo) -> Value) -> NativeResult {
    Ok(result(&reflected_class(this)?.info))
}

fn names(this: Option<&Rc<Object>>, list: impl FnOnce(&ClassInfo) -> &[String]) -> NativeResult {
    let class = reflected_class(this)?;
    values(
        list(&class.info)
            .iter()
            .map(|name| Value::from(name.as_str())),
    )
}

fn aliases(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let mut aliases = PhpArray::new();
    for (alias, original) in &reflected_class(this)?.info.trait_aliases {
        aliases.insert(
            ArrayKey::str(alias.as_str()),
            Value::from(original.as_str()),
        );
    }
    Ok(Value::from(aliases))
}

fn relations(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    select: fn(&ClassInfo) -> &[String],
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let class = reflected_class(this.as_ref())?;
        let mut result = PhpArray::new();
        for name in select(&class.info) {
            result.insert(
                ArrayKey::str(name.as_str()),
                reflect_class(context, name).await?,
            );
        }
        Ok(Value::from(result))
    })
}

fn parent(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let class = reflected_class(this.as_ref())?;
        match &class.info.parent {
            Some(name) => reflect_class(context, name).await,
            None => Ok(Value::Bool(false)),
        }
    })
}

#[derive(Clone, Copy)]
enum Relationship {
    Subclass,
    Interface,
}

fn relationship(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
    relationship: Relationship,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let class = reflected_class(this.as_ref())?;
        let other = match args.first() {
            Some(Value::Object(object)) if object.class.is_instance_of("ReflectionClass") => {
                reflected_class(Some(object))?.info
            }
            Some(Value::String(name)) => {
                let name = name.to_string_lossy();
                engine(context)?
                    .reflect_class(&name)
                    .await
                    .map_err(|error| match relationship {
                        Relationship::Interface
                            if matches!(
                                &error,
                                phera_extension_api::NativeError::Throw {
                                    class: "ReflectionException",
                                    ..
                                }
                            ) =>
                        {
                            exception(format!("Interface \"{name}\" does not exist"))
                        }
                        _ => error,
                    })?
            }
            _ => {
                let actual = args.first().map_or("null".into(), Value::type_name);
                let signature = match relationship {
                    Relationship::Subclass => "isSubclassOf(): Argument #1 ($class)",
                    Relationship::Interface => "implementsInterface(): Argument #1 ($interface)",
                };
                return Err(phera_extension_api::NativeError::type_error(format!(
                    "ReflectionClass::{signature} must be of type ReflectionClass|string, {actual} given"
                )));
            }
        };
        if matches!(relationship, Relationship::Interface)
            && other.kind != ReflectedClassKind::Interface
        {
            return Err(exception(format!("{} is not an interface", other.name)));
        }
        if class.info.name == other.name {
            return Ok(Value::Bool(matches!(relationship, Relationship::Interface)));
        }
        if class.info.interfaces.contains(&other.name) {
            return Ok(Value::Bool(true));
        }
        let mut parent = class.info.parent.clone();
        while let Some(name) = parent {
            if name == other.name {
                return Ok(Value::Bool(true));
            }
            parent = engine(context)?.reflect_class(&name).await?.parent.clone();
        }
        Ok(Value::Bool(false))
    })
}

fn instantiate(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Option<PhpArray>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let class = reflected_class(this.as_ref())?;
        engine(context)?
            .reflect_construct(&class.info, arguments)
            .await
    })
}
