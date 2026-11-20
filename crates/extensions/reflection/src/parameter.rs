//! `ReflectionParameter('handle', 'request')` selects a parameter by name or position.

use std::rc::Rc;

use phera_extension_api::reflection::{
    FunctionInfo, FunctionTarget, ParameterDefault, ParameterInfo,
};
use phera_extension_api::{
    ArrayKey, ClassEntry, Context, LocalFuture, MethodEntry, NativeResult, Object, Param,
    ParamType, Registry, Value,
};

use crate::class::{NAME_PROPERTY, reflect_class, target_class};
use crate::{UNCLONEABLE, allocate, bind, engine, exception, load, receiver};

#[derive(Clone)]
struct ReflectedParameter {
    function: Rc<FunctionInfo>,
    position: usize,
}

impl ReflectedParameter {
    fn info(&self) -> Result<&ParameterInfo, phera_extension_api::NativeError> {
        self.function
            .parameters
            .get(self.position)
            .ok_or_else(|| exception("The parameter specified by its offset could not be found"))
    }
}

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["Reflector"],
        properties: &[NAME_PROPERTY],
        methods: const {
            &[
                MethodEntry::public_async(
                    "__construct",
                    const {
                        &[
                            Param::new("function", ParamType::Mixed),
                            Param::new("param", ParamType::IntOrString),
                        ]
                    },
                    construct,
                ),
                MethodEntry::public("getName", &[], |_, o, _| {
                    data(o, |p| Value::from(p.name.as_str()))
                }),
                MethodEntry::public("getPosition", &[], |_, o, _| {
                    data(o, |p| Value::Int(p.position as i64))
                }),
                MethodEntry::public("hasType", &[], |_, o, _| {
                    data(o, |p| Value::Bool(p.declared_type.is_some()))
                }),
                MethodEntry::public("getType", &[], |c, o, _| {
                    crate::types::make(c, parameter(o)?.info()?.declared_type.as_deref())
                }),
                MethodEntry::public("allowsNull", &[], |_, o, _| {
                    data(o, |p| {
                        Value::Bool(
                            p.declared_type
                                .as_deref()
                                .is_none_or(crate::types::allows_null),
                        )
                    })
                }),
                MethodEntry::public("isOptional", &[], |_, o, _| {
                    data(o, |p| Value::Bool(p.optional))
                }),
                MethodEntry::public("isVariadic", &[], |_, o, _| {
                    data(o, |p| Value::Bool(p.variadic))
                }),
                MethodEntry::public("isPassedByReference", &[], |_, o, _| {
                    data(o, |p| Value::Bool(p.by_reference))
                }),
                MethodEntry::public("canBePassedByValue", &[], |_, o, _| {
                    data(o, |p| Value::Bool(!p.by_reference))
                }),
                MethodEntry::public("isPromoted", &[], |_, o, _| {
                    data(o, |p| Value::Bool(p.promoted))
                }),
                MethodEntry::public("isDefaultValueAvailable", &[], |_, o, _| {
                    data(o, |p| Value::Bool(p.default.is_some()))
                }),
                MethodEntry::public_async("getDefaultValue", &[], default_value),
                MethodEntry::public("isDefaultValueConstant", &[], |_, o, _| {
                    let parameter = parameter(o)?;
                    let parameter = require_default(&parameter)?;
                    Ok(Value::Bool(parameter.default_constant.is_some()))
                }),
                MethodEntry::public("getDefaultValueConstantName", &[], |_, o, _| {
                    let parameter = parameter(o)?;
                    let parameter = require_default(&parameter)?;
                    Ok(parameter
                        .default_constant
                        .as_ref()
                        .map_or(Value::Null, |name| Value::from(name.as_str())))
                }),
                MethodEntry::public("getDeclaringFunction", &[], |c, o, _| {
                    crate::function::make(c, parameter(o)?.function.target.clone())
                }),
                MethodEntry::public_async("getDeclaringClass", &[], declaring),
                MethodEntry::public_async("getClass", &[], legacy_class),
                MethodEntry::public("isArray", &[], |c, o, _| {
                    legacy_type(c, o, "isArray", "array")
                }),
                MethodEntry::public("isCallable", &[], |c, o, _| {
                    legacy_type(c, o, "isCallable", "callable")
                }),
                MethodEntry::public_async(
                    "getAttributes",
                    crate::attribute::FILTER,
                    |c, o, args| {
                        Box::pin(async move {
                            let parameter = parameter(o.as_ref())?;
                            crate::attribute::list(
                                c,
                                &parameter.info()?.attributes,
                                &args,
                                "ReflectionParameter",
                            )
                            .await
                        })
                    },
                ),
                MethodEntry::public("__toString", &[], |_, o, _| {
                    data(o, |p| {
                        Value::from(if p.internal {
                            crate::function::display::display_parameter(p)
                        } else {
                            format!("Parameter #{} [ ${} ]\n", p.position, p.name)
                        })
                    })
                })
                .returning("string"),
            ]
        },
        hooks: Some(&UNCLONEABLE),
        ..ClassEntry::class("ReflectionParameter")
    });
}

fn parameter(
    this: Option<&Rc<Object>>,
) -> Result<ReflectedParameter, phera_extension_api::NativeError> {
    load(this)
}

fn data(this: Option<&Rc<Object>>, read: impl FnOnce(&ParameterInfo) -> Value) -> NativeResult {
    Ok(read(parameter(this)?.info()?))
}

pub(crate) fn make(
    context: &mut dyn Context,
    function: Rc<FunctionInfo>,
    position: usize,
) -> NativeResult {
    let value = ReflectedParameter { function, position };
    let name = value.info()?.name.clone();
    let reflected = allocate(context, "ReflectionParameter", Some(&name), value.clone())?;
    if let Value::Object(object) = &reflected {
        crate::function::keep_closure(object, &value.function.target);
    }
    Ok(reflected)
}

fn construct(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let value = args.first().ok_or_else(|| exception("Missing function"))?;
        let target = function_target(value)?;
        if let FunctionTarget::Method { class, .. } = &target {
            engine(context)?.reflect_class(class).await?;
        }
        let function = engine(context)?.reflect_function(&target)?;
        let position = match args.get(1) {
            Some(Value::String(name)) => function
                .parameters
                .iter()
                .position(|p| name.as_bytes() == p.name.as_bytes())
                .ok_or_else(|| {
                    exception("The parameter specified by its name could not be found")
                })?,
            Some(Value::Int(position)) => usize::try_from(*position)
                .ok()
                .filter(|position| *position < function.parameters.len())
                .ok_or_else(|| {
                    exception("The parameter specified by its offset could not be found")
                })?,
            _ => {
                return Err(exception(
                    "The parameter specified by its offset could not be found",
                ));
            }
        };
        let state = ReflectedParameter { function, position };
        let name = state.info()?.name.clone();
        let object = receiver(this.as_ref())?;
        crate::function::keep_closure(object, &state.function.target);
        bind(object, Some(&name), state)
    })
}

fn function_target(value: &Value) -> Result<FunctionTarget, phera_extension_api::NativeError> {
    match value {
        Value::String(name) => Ok(FunctionTarget::Named(name.to_string_lossy())),
        Value::Object(object) if object.class.name() == "Closure" => {
            Ok(FunctionTarget::Closure(Rc::downgrade(object)))
        }
        Value::Object(object) => Ok(FunctionTarget::Method {
            class: object.class.name().into(),
            name: "__invoke".into(),
        }),
        Value::Array(array) => {
            let (Some(class), Some(Value::String(method))) =
                (array.get(&ArrayKey::Int(0)), array.get(&ArrayKey::Int(1)))
            else {
                return Err(exception(
                    "Expected array($object, $method) or array($classname, $method)",
                ));
            };
            Ok(crate::method::method_target(
                class,
                target_class(class)?,
                method.to_string_lossy(),
            ))
        }
        _ => Err(exception(format!(
            "ReflectionParameter::__construct(): Argument #1 ($function) must be a string, an array(class, method), or a callable object, {} given",
            value.type_name()
        ))),
    }
}

fn require_default(
    parameter: &ReflectedParameter,
) -> Result<&ParameterInfo, phera_extension_api::NativeError> {
    let info = parameter.info()?;
    if info.default.is_none() {
        return Err(exception(
            "Internal error: Failed to retrieve the default value",
        ));
    }
    Ok(info)
}

fn default_value(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let parameter = parameter(this.as_ref())?;
        let info = require_default(&parameter)?;
        match info.default.clone() {
            Some(ParameterDefault::Literal(Value::Int(number)))
                if !info.internal
                    && info.declared_type.as_deref().is_some_and(default_is_float) =>
            {
                Ok(Value::Float(number as f64))
            }
            Some(ParameterDefault::Literal(value)) => Ok(value),
            Some(ParameterDefault::Expression(initializer)) => {
                engine(context)?.reflect_evaluate(initializer).await
            }
            Some(ParameterDefault::NativeEnumCase { class, case }) => {
                engine(context)?.reflect_class_constant(class, case)
            }
            None => Err(exception(
                "Internal error: Failed to retrieve the default value",
            )),
        }
    })
}

fn default_is_float(ty: &str) -> bool {
    let mut members = ty.trim_start_matches('?').split('|');
    members.clone().any(|member| member == "float") && !members.any(|member| member == "int")
}

fn declaring(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let parameter = parameter(this.as_ref())?;
        match &parameter.function.member {
            Some(member) => reflect_class(context, &member.class).await,
            None => Ok(Value::Null),
        }
    })
}

fn legacy_class(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        deprecation(context, "getClass");
        let parameter = parameter(this.as_ref())?;
        let Some(declared) = parameter.info()?.declared_type.as_deref() else {
            return Ok(Value::Null);
        };
        let declared = declared.trim_start_matches('?');
        match declared {
            "self" => match &parameter.function.member {
                Some(member) => reflect_class(context, &member.class).await,
                None => Ok(Value::Null),
            },
            "parent" => {
                if let Some(member) = &parameter.function.member
                    && let Some(parent) = engine(context)?
                        .reflect_class(&member.class)
                        .await?
                        .parent
                        .clone()
                {
                    return reflect_class(context, &parent).await;
                }
                Ok(Value::Null)
            }
            _ if crate::types::is_builtin(declared) || declared.contains(['|', '&']) => {
                Ok(Value::Null)
            }
            _ => reflect_class(context, declared).await,
        }
    })
}

fn legacy_type(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    method: &str,
    ty: &str,
) -> NativeResult {
    deprecation(context, method);
    data(this, |parameter| {
        Value::Bool(
            parameter
                .declared_type
                .as_deref()
                .is_some_and(|declared| declared.trim_start_matches('?') == ty),
        )
    })
}

fn deprecation(context: &mut dyn Context, method: &str) {
    context.deprecated(format!(
        "Method ReflectionParameter::{method}() is deprecated since 8.0, use ReflectionParameter::getType() instead"
    ));
}
