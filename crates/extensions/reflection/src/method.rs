//! `ReflectionMethod::invoke($object, $argument)` can call a private method.

use std::rc::Rc;

use phera_extension_api::reflection::FunctionTarget;
use phera_extension_api::{
    ClassEntry, ConstantValue, Context, LocalFuture, MethodEntry, NativeResult, Object, Param,
    ParamType, Registry, Value, args,
};

use crate::class::{reflect_class, target_class};
use crate::function::{
    ARGS, ARRAY, argument_array, data, make, method, positional, reflected, set_class,
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
                ),
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
                    Ok(Value::Bool(
                        engine(c)?
                            .reflect_prototype(method(&*reflected(o)?)?)
                            .is_some(),
                    ))
                }),
                MethodEntry::public("getPrototype", &[], prototype),
                MethodEntry::public("setAccessible", ACCESS, |c, o, _| {
                    reflected(o)?;
                    accessible(c, "ReflectionMethod")
                }),
                MethodEntry::public(
                    "getClosure",
                    const { &[INSTANCE.with_default(ConstantValue::Null)] },
                    |c, o, a| {
                        engine(c)?.reflect_closure(
                            reflected(o)?.target.clone(),
                            args::object(a, 0).cloned(),
                        )
                    },
                ),
                MethodEntry::public_async("invoke", &[INSTANCE, ARGS], |c, o, a| {
                    invoke(c, o, a, Arguments::Positional)
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
            Some(name) => (target_class(first)?, name.to_string_lossy()),
            None => {
                context.deprecated("Calling ReflectionMethod::__construct() with 1 argument is deprecated, use ReflectionMethod::createFromMethodName() instead".into());
                match first {
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
                }
            }
        };
        engine(context)?.reflect_class(&class).await?;
        let function =
            Rc::new(engine(context)?.reflect_function(&FunctionTarget::Method { class, name })?);
        set_class(this, method(&function)?);
        bind(this, Some(&function.name.clone()), function)
    })
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
    let prototype = engine(context)?.reflect_prototype(member).ok_or_else(|| {
        exception(format!(
            "Method {}::{} does not have a prototype",
            member.class, member.name
        ))
    })?;
    make(
        context,
        FunctionTarget::Method {
            class: prototype.class,
            name: prototype.name,
        },
    )
}

enum Arguments {
    Array,
    Positional,
}

fn invoke(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
    mode: Arguments,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = reflected(this.as_ref())?;
        let object = args::object(&args, 0).cloned();
        let arguments = match mode {
            Arguments::Array => argument_array(&args, 1),
            Arguments::Positional => positional(args.into_iter().skip(1))?,
        };
        engine(context)?
            .reflect_invoke(function.target.clone(), object, arguments)
            .await
    })
}
