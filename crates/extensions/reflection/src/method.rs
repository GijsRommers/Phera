//! `ReflectionMethod::invoke($object, $argument)` can call a private method.

use std::rc::Rc;

use phera_extension_api::reflection::FunctionTarget;
use phera_extension_api::{
    ClassEntry, ConstantValue, Context, LocalFuture, MethodEntry, NativeResult, Object, Param,
    ParamType, PhpString, Registry, Value, args,
};

use crate::class::{reflect_class, target_class};
use crate::function::{
    ARGS, ARRAY, argument_array, data, forwarded_arguments, make, method, reflected, set_class,
};
use crate::member::{ACCESS, CLASS_PROPERTY, accessible};
use crate::{bind, engine, exception, receiver};

const INSTANCE: Param = Param::new("object", ParamType::Object).nullable();

pub(crate) fn register(registry: &mut Registry) {
    use ConstantValue::Int;
    registry.class(ClassEntry {
        parent: Some("ReflectionFunctionAbstract"),
        properties: &[CLASS_PROPERTY],
        constants: &[
            ("IS_STATIC", Int(16)),
            ("IS_PUBLIC", Int(1)),
            ("IS_PROTECTED", Int(2)),
            ("IS_PRIVATE", Int(4)),
            ("IS_ABSTRACT", Int(64)),
            ("IS_FINAL", Int(32)),
        ],
        methods: const {
            &[
                MethodEntry::public_async(
                    "__construct",
                    const {
                        &[
                            Param::new("objectOrMethod", ParamType::ObjectOrString),
                            Param::new("method", ParamType::String)
                                .nullable()
                                .with_default(ConstantValue::Null),
                        ]
                    },
                    construct,
                )
                .with_preparation(|context, _, arguments| {
                    if arguments.len() == 1 {
                        context.deprecated("Calling ReflectionMethod::__construct() with 1 argument is deprecated, use ReflectionMethod::createFromMethodName() instead".into());
                    }
                    Ok(phera_extension_api::MethodPreparation::Default)
                }),
                MethodEntry::public_async(
                    "createFromMethodName",
                    const { &[Param::new("method", ParamType::String)] },
                    create_from_method_name,
                )
                .static_()
                .returning("static"),
                MethodEntry::public("isPublic", &[], |_, o, _| flags(o, 1)),
                MethodEntry::public("isProtected", &[], |_, o, _| flags(o, 2)),
                MethodEntry::public("isPrivate", &[], |_, o, _| flags(o, 4)),
                MethodEntry::public("isFinal", &[], |_, o, _| flags(o, 32)),
                MethodEntry::public("isAbstract", &[], |_, o, _| flags(o, 64)),
                MethodEntry::public("isConstructor", &[], |_, o, _| {
                    data(o, |f| {
                        Value::Bool(f.name.eq_ignore_ascii_case("__construct"))
                    })
                }),
                MethodEntry::public("isDestructor", &[], |_, o, _| {
                    data(o, |f| {
                        Value::Bool(f.name.eq_ignore_ascii_case("__destruct"))
                    })
                }),
                MethodEntry::public("getModifiers", &[], |_, o, _| {
                    Ok(Value::Int(method(&*reflected(o)?)?.modifiers))
                }),
                MethodEntry::public_async("getDeclaringClass", &[], declaring),
                MethodEntry::public("hasPrototype", &[], |c, o, _| {
                    Ok(Value::Bool(prototype_target(c, &*reflected(o)?)?.is_some()))
                }),
                MethodEntry::public("getPrototype", &[], prototype),
                MethodEntry::public("setAccessible", ACCESS, |_, o, _| {
                    reflected(o)?;
                    Ok(Value::Null)
                }).with_preparation(|c, _, _| accessible(c, "ReflectionMethod")),
                MethodEntry::public(
                    "getClosure",
                    const { &[INSTANCE.with_default(ConstantValue::Null)] },
                    |c, o, a| {
                        let function = reflected(o)?;
                        let member = method(&function)?;
                        let object = args::object(a, 0).cloned();
                        if !function.is_static && object.is_none() {
                            return Err(phera_extension_api::NativeError::value_error(
                                "ReflectionMethod::getClosure(): Argument #1 ($object) cannot be null for non-static methods",
                            ));
                        }
                        if !function.is_static
                            && object.as_ref().is_some_and(|object| {
                                !object.class.is_instance_of(&member.class)
                            })
                        {
                            return Err(exception(
                                "Given object is not an instance of the class this method was declared in",
                            ));
                        }
                        let target = match &function.target {
                            FunctionTarget::ClosureInvoke(_) | FunctionTarget::PropertyHook { .. } => function.target.clone(),
                            _ => FunctionTarget::Method {
                                class: member.class.clone(),
                                name: member.name.clone(),
                            },
                        };
                        engine(c)?.reflect_closure(target, object)
                    },
                ),
                MethodEntry::public_async_named("invoke", &[INSTANCE, ARGS], |c, o, a, names| {
                    invoke(c, o, a, Arguments::Forwarded(names))
                }),
                MethodEntry::public_async("invokeArgs", &[INSTANCE, ARRAY], |c, o, a| {
                    invoke(c, o, a, Arguments::Array)
                }),
                MethodEntry::public("__toString", &[], |_, o, _| {
                    let f = reflected(o)?;
                    let m = method(&f)?;
                    Ok(Value::from(format!("Method [ {}::{} ]\n", m.class, m.name)))
                })
                .returning("string"),
            ]
        },
        ..ClassEntry::class("ReflectionMethod")
    });
}

fn construct(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let this = receiver(this.as_ref())?;
        let first = arguments
            .first()
            .ok_or_else(|| exception("Expected a class or method name"))?;
        let (class, name) = match args::string(&arguments, 1) {
            Some(name) => (target_class(first)?.into_owned(), name.to_string_lossy()),
            None => match first {
                Value::String(name) => split_method(
                    &name.to_string_lossy(),
                    "ReflectionMethod::__construct()",
                    "objectOrMethod",
                )?,
                _ => {
                    return Err(phera_extension_api::NativeError::throw(
                        "ArgumentCountError",
                        "ReflectionMethod::__construct() expects exactly 2 arguments, 1 given",
                    ));
                }
            },
        };
        engine(context)?.reflect_class(&class).await?;
        let target = method_target(first, class, name);
        let function = engine(context)?.reflect_function(&target)?;
        crate::function::keep_closure(this, &target);
        set_class(this, method(&function)?);
        bind(this, Some(&function.name.clone()), function)
    })
}

pub(crate) fn method_target(receiver: &Value, class: String, name: String) -> FunctionTarget {
    if name.eq_ignore_ascii_case("__invoke")
        && let Value::Object(closure) = receiver
        && closure.class.is_instance_of("Closure")
    {
        FunctionTarget::ClosureInvoke(Rc::downgrade(closure))
    } else {
        FunctionTarget::Method { class, name }
    }
}

fn create_from_method_name(
    context: &mut dyn Context,
    _: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let text = args::string(&arguments, 0)
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        let (class, name) =
            split_method(&text, "ReflectionMethod::createFromMethodName()", "method")?;
        engine(context)?.reflect_class(&class).await?;
        make(context, FunctionTarget::Method { class, name })
    })
}

pub(crate) fn split_method(
    name: &str,
    function: &str,
    parameter: &str,
) -> Result<(String, String), phera_extension_api::NativeError> {
    name.split_once("::")
        .map(|(class, name)| (class.to_owned(), name.to_owned()))
        .ok_or_else(|| {
            exception(format!(
                "{function}: Argument #1 (${parameter}) must be a valid method name"
            ))
        })
}

fn flags(this: Option<&Rc<Object>>, flag: i64) -> NativeResult {
    Ok(Value::Bool(
        method(&*reflected(this)?)?.modifiers & flag != 0,
    ))
}

fn declaring(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = reflected(this.as_ref())?;
        reflect_class(context, &method(&function)?.class).await
    })
}

fn prototype(context: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let function = reflected(this)?;
    let member = method(&function)?;
    let prototype = prototype_target(context, &function)?.ok_or_else(|| {
        exception(format!(
            "Method {}::{} does not have a prototype",
            member.class, member.name
        ))
    })?;
    make(context, prototype)
}

fn prototype_target(
    context: &mut dyn Context,
    function: &phera_extension_api::reflection::FunctionInfo,
) -> Result<Option<FunctionTarget>, phera_extension_api::NativeError> {
    let engine = engine(context)?;
    Ok(match function.target {
        FunctionTarget::PropertyHook { property, kind } => {
            engine.reflect_hook_prototype(property, kind)
        }
        _ => engine
            .reflect_prototype(method(function)?)
            .map(|prototype| FunctionTarget::Method {
                class: prototype.class,
                name: prototype.name,
            }),
    })
}

enum Arguments<'a> {
    Array,
    Forwarded(&'a [(usize, PhpString)]),
}

fn invoke<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
    mode: Arguments<'a>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let function = reflected(this.as_ref())?;
        let object = args::object(&args, 0).cloned();
        let arguments = match mode {
            Arguments::Array => argument_array(&args, 1),
            Arguments::Forwarded(names) => forwarded_arguments(args, 1, names)?,
        };
        engine(context)?
            .reflect_invoke(function.target.clone(), object, arguments)
            .await
    })
}
