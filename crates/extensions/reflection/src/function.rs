//! `ReflectionFunction` and `ReflectionMethod` share callable metadata.

use std::rc::Rc;

use phera_extension_api::reflection::{FunctionInfo, FunctionTarget, MemberInfo};
use phera_extension_api::{
    ArrayKey, ClassEntry, ClassKind, ConstantValue, Context, LocalFuture, MethodEntry,
    NativeResult, Object, Param, ParamType, PhpArray, PhpString, Registry, Value,
};

use crate::class::{NAME_PROPERTY, reflect_class, split};
pub(crate) mod display;
use crate::{UNCLONEABLE, allocate, bind, engine, exception, load, receiver, values};

pub(crate) const ARGS: Param = Param::new("args", ParamType::Mixed).variadic();
pub(crate) const ARRAY: Param = Param::new("args", ParamType::Array);

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        kind: ClassKind::AbstractClass,
        interfaces: &["Reflector"],
        properties: &[NAME_PROPERTY],
        methods: ABSTRACT_METHODS,
        hooks: Some(&UNCLONEABLE),
        ..ClassEntry::class("ReflectionFunctionAbstract")
    });
    registry.class(ClassEntry {
        parent: Some("ReflectionFunctionAbstract"),
        constants: &[("IS_DEPRECATED", ConstantValue::Int(2048))],
        methods: const {
            &[
                MethodEntry::public(
                    "__construct",
                    const { &[Param::new("function", ParamType::ClosureOrString)] },
                    construct,
                )
                .with_ready(|c, o, a| Some(construct(c, Some(o), a))),
                MethodEntry::public("isAnonymous", &[], |_, o, _| {
                    data(o, |f| Value::Bool(f.anonymous))
                }),
                MethodEntry::public_async_named("invoke", &[ARGS], |c, o, a, names| {
                    Box::pin(async move {
                        let function = reflected(o.as_ref())?;
                        let args = forwarded_arguments(a, 0, names)?;
                        engine(c)?
                            .reflect_invoke(function.target.clone(), None, args)
                            .await
                    })
                })
                .tentative_return("mixed"),
                MethodEntry::public_async("invokeArgs", &[ARRAY], |c, o, a| {
                    Box::pin(async move {
                        let function = reflected(o.as_ref())?;
                        let args = argument_array(&a, 0);
                        engine(c)?
                            .reflect_invoke(function.target.clone(), None, args)
                            .await
                    })
                })
                .tentative_return("mixed"),
                MethodEntry::public("getClosure", &[], |c, o, _| {
                    engine(c)?.reflect_closure(reflected(o)?.target.clone(), None)
                })
                .returning("Closure"),
                MethodEntry::public("__toString", &[], |_, o, _| {
                    data(o, |f| Value::from(display::function(f)))
                })
                .returning("string"),
            ]
        },
        ..ClassEntry::class("ReflectionFunction")
    });
    crate::method::register(registry);
}

const ABSTRACT_METHODS: &[MethodEntry] = &[
    MethodEntry::public("getName", &[], |_, o, _| {
        data(o, |f| Value::from(f.name.as_str()))
    })
    .with_direct(direct_get_name),
    MethodEntry::public("getShortName", &[], |_, o, _| {
        data(o, |f| Value::from(name_parts(f).1))
    }),
    MethodEntry::public("getNamespaceName", &[], |_, o, _| {
        data(o, |f| Value::from(name_parts(f).0))
    }),
    MethodEntry::public("inNamespace", &[], |_, o, _| {
        data(o, |f| Value::Bool(!name_parts(f).0.is_empty()))
    }),
    MethodEntry::public("isClosure", &[], |_, o, _| {
        data(o, |f| Value::Bool(f.is_closure))
    })
    .with_direct(direct_is_closure),
    MethodEntry::public("isDeprecated", &[], |_, o, _| {
        data(o, |f| Value::Bool(f.deprecated))
    })
    .with_direct(direct_is_deprecated),
    MethodEntry::public("isInternal", &[], |_, o, _| {
        data(o, |f| Value::Bool(f.source.is_none()))
    })
    .with_direct(direct_is_internal),
    MethodEntry::public("isUserDefined", &[], |_, o, _| {
        data(o, |f| Value::Bool(f.source.is_some()))
    })
    .with_direct(direct_is_user_defined),
    MethodEntry::public("isGenerator", &[], |_, o, _| {
        data(o, |f| Value::Bool(f.is_generator))
    })
    .with_direct(direct_is_generator),
    MethodEntry::public("isStatic", &[], |_, o, _| {
        data(o, |f| Value::Bool(f.is_static))
    })
    .with_direct(direct_is_static),
    MethodEntry::public("isVariadic", &[], |_, o, _| {
        data(o, |f| Value::Bool(f.parameters.iter().any(|p| p.variadic)))
    }),
    MethodEntry::public("getClosureThis", &[], |_, o, _| {
        data(o, |f| {
            f.closure_this
                .as_ref()
                .and_then(std::rc::Weak::upgrade)
                .map_or(Value::Null, Value::Object)
        })
    }),
    MethodEntry::public_async("getClosureScopeClass", &[], |c, o, _| {
        class(c, o, |f| f.closure_scope.as_deref())
    }),
    MethodEntry::public_async("getClosureCalledClass", &[], |c, o, _| {
        class(c, o, |f| f.closure_called.as_deref())
    }),
    MethodEntry::public("getClosureUsedVariables", &[], |c, o, _| {
        engine(c)?.reflect_closure_uses(&reflected(o)?.target)
    }),
    MethodEntry::public("getFileName", &[], |_, o, _| {
        data(o, |f| {
            f.source
                .as_ref()
                .map_or(Value::Bool(false), |s| Value::from(s.file.as_str()))
        })
    }),
    MethodEntry::public("getDocComment", &[], |_, o, _| {
        data(o, |f| {
            f.source
                .as_ref()
                .and_then(|s| s.doc_comment.as_ref())
                .map_or(Value::Bool(false), |doc| Value::from(doc.clone()))
        })
    }),
    MethodEntry::public("getStartLine", &[], |_, o, _| {
        data(o, |f| {
            f.source
                .as_ref()
                .map_or(Value::Bool(false), |s| Value::Int(s.start_line.into()))
        })
    }),
    MethodEntry::public("getEndLine", &[], |_, o, _| {
        data(o, |f| {
            f.source
                .as_ref()
                .map_or(Value::Bool(false), |s| Value::Int(s.end_line.into()))
        })
    }),
    MethodEntry::public("getExtensionName", &[], |_, o, _| {
        data(o, |f| {
            f.extension
                .as_ref()
                .map_or(Value::Bool(false), |s| Value::from(s.as_str()))
        })
    }),
    MethodEntry::public("getExtension", &[], |c, o, _| {
        match &reflected(o)?.extension {
            Some(name) => crate::extension::create(c, name),
            None => Ok(Value::Null),
        }
    }),
    MethodEntry::public("getNumberOfParameters", &[], |_, o, _| {
        data(o, |f| Value::Int(f.parameters.len() as i64))
    }),
    MethodEntry::public("getNumberOfRequiredParameters", &[], |_, o, _| {
        data(o, |f| {
            Value::Int(f.parameters.iter().filter(|p| !p.optional).count() as i64)
        })
    }),
    MethodEntry::public("getParameters", &[], parameters),
    MethodEntry::public("getStaticVariables", &[], |c, o, _| {
        engine(c)?.reflect_statics(&reflected(o)?.target)
    }),
    MethodEntry::public("returnsReference", &[], |_, o, _| {
        data(o, |f| Value::Bool(f.returns_reference))
    })
    .with_direct(direct_returns_reference),
    MethodEntry::public("hasReturnType", &[], |_, o, _| {
        data(o, |f| Value::Bool(f.return_type.is_some()))
    })
    .with_direct(direct_has_return_type),
    MethodEntry::public("getReturnType", &[], |c, o, _| {
        crate::types::make(c, reflected(o)?.return_type.as_deref())
    })
    .with_direct(direct_no_return_type),
    MethodEntry::public("hasTentativeReturnType", &[], |_, o, _| {
        data(o, |f| Value::Bool(f.tentative_return_type.is_some()))
    }),
    MethodEntry::public("getTentativeReturnType", &[], |c, o, _| {
        crate::types::make(c, reflected(o)?.tentative_return_type.as_deref())
    }),
    MethodEntry::public_async("getAttributes", crate::attribute::FILTER, attributes)
        .with_direct(direct_no_attributes),
];

pub(crate) fn make(context: &mut dyn Context, target: FunctionTarget) -> NativeResult {
    let function = engine(context)?.reflect_function(&target)?;
    let name = function.name.clone();
    let class = if function.member.is_some() {
        "ReflectionMethod"
    } else {
        "ReflectionFunction"
    };
    let value = allocate(context, class, Some(&name), function.clone())?;
    if let Value::Object(object) = &value {
        keep_closure(object, &function.target);
        if let Some(member) = &function.member {
            set_class(object, member);
        }
    }
    Ok(value)
}

pub(crate) fn set_class(object: &Object, member: &MemberInfo) {
    object.set(ArrayKey::str("class"), Value::from(member.class.as_str()));
}

fn construct(context: &mut dyn Context, this: Option<&Rc<Object>>, args: &[Value]) -> NativeResult {
    let target = match args.first() {
        Some(Value::Object(object)) if object.class.name() == "Closure" => {
            FunctionTarget::Closure(Rc::downgrade(object))
        }
        Some(Value::InternedStr(name)) => FunctionTarget::Named(name.to_string_lossy()),
        Some(Value::String(name)) => FunctionTarget::Named(name.to_string_lossy()),
        other => {
            return Err(phera_extension_api::NativeError::type_error(format!(
                "ReflectionFunction::__construct(): Argument #1 ($function) must be of type Closure|string, {} given",
                other.map_or("null".into(), Value::type_name)
            )));
        }
    };
    let function = engine(context)?.reflect_function(&target)?;
    let object = receiver(this)?;
    keep_closure(object, &function.target);
    bind(object, Some(&function.name), function.clone())
}

pub(crate) fn keep_closure(object: &Object, target: &FunctionTarget) {
    match target {
        FunctionTarget::Closure(closure) | FunctionTarget::ClosureInvoke(closure) => {
            if let Some(closure) = closure.upgrade() {
                crate::keep(object, Value::Object(closure));
            }
        }
        _ => {
            object.remove(&ArrayKey::str(crate::RETAINED));
        }
    }
}

pub(crate) fn reflected(
    this: Option<&Rc<Object>>,
) -> Result<Rc<FunctionInfo>, phera_extension_api::NativeError> {
    load(this)
}

pub(crate) fn data(
    this: Option<&Rc<Object>>,
    result: impl FnOnce(&FunctionInfo) -> Value,
) -> NativeResult {
    Ok(result(&*reflected(this)?))
}

fn name_parts(function: &FunctionInfo) -> (&str, &str) {
    if function.anonymous || function.member.is_some() {
        ("", function.name.as_str())
    } else {
        split(&function.name)
    }
}

fn parameters(context: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let function = reflected(this)?;
    let mut result = Vec::new();
    for position in 0..function.parameters.len() {
        result.push(crate::parameter::make(context, function.clone(), position)?);
    }
    values(result)
}

fn class(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    select: fn(&FunctionInfo) -> Option<&str>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = reflected(this.as_ref())?;
        match select(&function) {
            Some(name) => reflect_class(context, name).await,
            None => Ok(Value::Null),
        }
    })
}

fn attributes(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = reflected(this.as_ref())?;
        crate::attribute::list(
            context,
            &function.attributes,
            &arguments,
            "ReflectionFunctionAbstract",
        )
        .await
    })
}

pub(crate) fn forwarded_arguments(
    args: Vec<Value>,
    skip: usize,
    named: &[(usize, PhpString)],
) -> Result<PhpArray, phera_extension_api::NativeError> {
    let mut array = PhpArray::new();
    let mut named = named.iter().peekable();
    for (index, argument) in args.into_iter().enumerate().skip(skip) {
        if named.peek().is_some_and(|(position, _)| *position == index)
            && let Some((_, name)) = named.next()
        {
            array.insert(ArrayKey::String(name.clone()), argument);
        } else {
            array.push(argument)?;
        }
    }
    Ok(array)
}

pub(crate) fn argument_array(args: &[Value], index: usize) -> PhpArray {
    phera_extension_api::args::array(args, index)
        .map(|array| (**array).clone())
        .unwrap_or_default()
}

pub(crate) fn method(
    function: &FunctionInfo,
) -> Result<&MemberInfo, phera_extension_api::NativeError> {
    function
        .member
        .as_ref()
        .ok_or_else(|| exception("Not a reflected method"))
}

/// A getter run by the interpreter loop itself (a direct method): it reads
/// the reflected function from the object's state, and gives `None`, for
/// the ordinary call to raise PHP's error, when arguments were passed.
fn direct(
    object: &Object,
    arguments: &[Option<Value>],
    read: impl FnOnce(&FunctionInfo) -> Value,
) -> Option<Value> {
    if !arguments.is_empty() {
        return None;
    }
    let native = object.native.try_borrow().ok()?;
    let function = native.as_ref()?.downcast_ref::<Rc<FunctionInfo>>()?;
    Some(read(function))
}

fn direct_get_name(object: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    direct(object, arguments, |f| Value::from(f.name.as_str()))
}

fn direct_is_closure(object: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    direct(object, arguments, |f| Value::Bool(f.is_closure))
}

fn direct_is_deprecated(object: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    direct(object, arguments, |f| Value::Bool(f.deprecated))
}

fn direct_is_internal(object: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    direct(object, arguments, |f| Value::Bool(f.source.is_none()))
}

fn direct_is_user_defined(object: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    direct(object, arguments, |f| Value::Bool(f.source.is_some()))
}

fn direct_is_generator(object: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    direct(object, arguments, |f| Value::Bool(f.is_generator))
}

fn direct_is_static(object: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    direct(object, arguments, |f| Value::Bool(f.is_static))
}

fn direct_returns_reference(object: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    direct(object, arguments, |f| Value::Bool(f.returns_reference))
}

fn direct_has_return_type(object: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    direct(object, arguments, |f| Value::Bool(f.return_type.is_some()))
}

/// `getAttributes()` of a function with no attributes: an empty array, for
/// no filter or a class name with no flags (the flags `IS_INSTANCEOF` look
/// the class up, and others are an error: the ordinary call).
fn direct_no_attributes(object: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    let plain = matches!(
        arguments,
        [] | [Some(Value::InternedStr(_) | Value::String(_) | Value::Null)]
            | [
                Some(Value::InternedStr(_) | Value::String(_) | Value::Null),
                Some(Value::Int(0))
            ]
    );
    if !plain {
        return None;
    }
    let native = object.native.try_borrow().ok()?;
    let function = native.as_ref()?.downcast_ref::<Rc<FunctionInfo>>()?;
    function
        .attributes
        .is_empty()
        .then(|| Value::from(phera_extension_api::PhpArray::new()))
}

/// `getReturnType()` of a function with no declared return type: null. A
/// declared one is an object (the ordinary call).
fn direct_no_return_type(object: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    if !arguments.is_empty() {
        return None;
    }
    let native = object.native.try_borrow().ok()?;
    let function = native.as_ref()?.downcast_ref::<Rc<FunctionInfo>>()?;
    function.return_type.is_none().then_some(Value::Null)
}
