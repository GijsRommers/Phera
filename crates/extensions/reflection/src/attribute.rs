//! `getAttributes()` reveals declaration names before any argument or attribute class runs.

use std::rc::Rc;

use phera_extension_api::reflection::{
    AttributeInfo, AttributeTarget, AttributeValue, builtin_flags, target_names,
};
use phera_extension_api::{
    ArrayKey, ClassEntry, ConstantValue, Context, LocalFuture, MethodEntry, NativeError,
    NativeResult, Object, Param, ParamType, PhpArray, Registry, Value, Visibility, args,
};

use crate::{UNCLONEABLE, allocate, engine, load, values};

pub(crate) const FILTER: &[Param] = const {
    &[
        Param::new("name", ParamType::String)
            .nullable()
            .with_default(ConstantValue::Null),
        Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0)),
    ]
};
const INSTANCEOF: i64 = 2;
const REPEATABLE: i64 = 128;

#[derive(Clone)]
struct ReflectedAttribute {
    info: Rc<AttributeInfo>,
    repeated: bool,
}

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["Reflector"],
        properties: &[crate::class::NAME_PROPERTY],
        constants: const { &[("IS_INSTANCEOF", ConstantValue::Int(INSTANCEOF))] },
        methods: const {
            &[
                MethodEntry::public("__construct", &[], |_, _, _| Ok(Value::Null))
                    .with_visibility(Visibility::Private),
                MethodEntry::public("getName", &[], |_, o, _| {
                    Ok(Value::from(attribute(o)?.info.name.as_str()))
                })
                .returning("string"),
                MethodEntry::public("getTarget", &[], |_, o, _| {
                    Ok(Value::Int(attribute(o)?.info.target as i64))
                })
                .returning("int"),
                MethodEntry::public("isRepeated", &[], |_, o, _| {
                    Ok(Value::Bool(attribute(o)?.repeated))
                })
                .returning("bool"),
                MethodEntry::public_async("getArguments", &[], |c, o, _| {
                    Box::pin(async move {
                        let info = attribute(o.as_ref())?;
                        Ok(Value::from(arguments(c, &info.info).await?))
                    })
                })
                .returning("array"),
                MethodEntry::public_async("newInstance", &[], instantiate).returning("object"),
                MethodEntry::public("__toString", &[], |_, o, _| {
                    Ok(Value::from(format!(
                        "Attribute [ {} ]\n",
                        attribute(o)?.info.name
                    )))
                })
                .returning("string"),
            ]
        },
        hooks: Some(&UNCLONEABLE),
        ..ClassEntry::class("ReflectionAttribute")
    });
}

fn attribute(this: Option<&Rc<Object>>) -> Result<ReflectedAttribute, NativeError> {
    load(this)
}

pub(crate) async fn class_attributes(
    context: &mut dyn Context,
    class: &phera_extension_api::reflection::ClassInfo,
    args: &[Value],
) -> NativeResult {
    if class.name == "stdClass" {
        let marker = AttributeInfo {
            validation_error: None,
            site: None,
            name: "AllowDynamicProperties".into(),
            target: AttributeTarget::Class,
            arguments: Vec::new(),
        };
        return list(context, &[marker], args, "ReflectionClass").await;
    }
    if let Some(flags) =
        builtin_flags(&class.name).filter(|_| class.extension.as_deref() == Some("Core"))
    {
        let marker = AttributeInfo {
            validation_error: None,
            site: None,
            name: "Attribute".into(),
            target: AttributeTarget::Class,
            arguments: vec![(None, AttributeValue::Literal(Value::Int(flags)))],
        };
        return list(context, &[marker], args, "ReflectionClass").await;
    }
    list(context, &class.attributes, args, "ReflectionClass").await
}

pub(crate) async fn list(
    context: &mut dyn Context,
    attributes: &[AttributeInfo],
    args: &[Value],
    reflector: &str,
) -> NativeResult {
    let flags = args::int(args, 1).unwrap_or_default();
    if flags != 0 && flags != INSTANCEOF {
        return Err(NativeError::value_error(format!(
            "{reflector}::getAttributes(): Argument #2 ($flags) must be a valid attribute filter flag"
        )));
    }
    let name = args::string(args, 0).map(|name| name.to_string_lossy());
    if flags == INSTANCEOF
        && let Some(name) = &name
    {
        lookup(context, name, format!("Class \"{name}\" not found")).await?;
    }
    let mut result = Vec::new();
    for info in attributes {
        let matches = match &name {
            None => true,
            Some(name) if flags == 0 => name.eq_ignore_ascii_case(&info.name),
            Some(name) => is_instance_of(context, &info.name, name).await?,
        };
        if matches {
            let repeated = attributes
                .iter()
                .filter(|item| info.name.eq_ignore_ascii_case(&item.name))
                .take(2)
                .count()
                > 1;
            let state = ReflectedAttribute {
                info: Rc::new(info.clone()),
                repeated,
            };
            result.push(allocate(
                context,
                "ReflectionAttribute",
                Some(&info.name),
                state,
            )?);
        }
    }
    values(result)
}

async fn is_instance_of(
    context: &mut dyn Context,
    class: &str,
    target: &str,
) -> Result<bool, NativeError> {
    let mut next = Some(class.to_owned());
    while let Some(name) = next {
        let info = match engine(context)?.reflect_class(&name).await {
            Ok(class) => class,
            Err(error) if missing_class(&error) => return Ok(false),
            Err(error) => return Err(error),
        };
        if info
            .name
            .eq_ignore_ascii_case(target.trim_start_matches('\\'))
            || info
                .interfaces
                .iter()
                .any(|name| name.eq_ignore_ascii_case(target.trim_start_matches('\\')))
        {
            return Ok(true);
        }
        next = info.parent;
    }
    Ok(false)
}

fn instantiate(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let attr = attribute(this.as_ref())?;
        let name = &attr.info.name;
        let class = lookup(
            context,
            name,
            format!("Attribute class \"{name}\" not found"),
        )
        .await?;
        let marker = class
            .attributes
            .iter()
            .find(|info| info.name.eq_ignore_ascii_case("Attribute"));
        let flags = if let Some(flags) =
            builtin_flags(&class.name).filter(|_| class.extension.as_deref() == Some("Core"))
        {
            flags
        } else if let Some(marker) = marker {
            marker_flags(context, marker).await?
        } else {
            return Err(NativeError::error(format!(
                "Attempting to use non-attribute class \"{name}\" as attribute"
            )));
        };
        if flags & attr.info.target as i64 == 0 {
            let targets = target_names(flags);
            return Err(NativeError::error(format!(
                "Attribute \"{name}\" cannot target {} (allowed targets: {targets})",
                attr.info.target.name()
            )));
        }
        if let Some(error) = &attr.info.validation_error {
            return Err(NativeError::error(error.clone()));
        }
        if attr.repeated && flags & REPEATABLE == 0 {
            return Err(NativeError::error(format!(
                "Attribute \"{name}\" must not be repeated"
            )));
        }
        let values = arguments(context, &attr.info).await?;
        engine(context)?
            .reflect_construct_attribute(&class.name, values, attr.info.site.clone())
            .await
    })
}

/// Zend reads only the marker's first stored argument and requires an integer.
async fn marker_flags(
    context: &mut dyn Context,
    marker: &AttributeInfo,
) -> Result<i64, NativeError> {
    let Some((_, argument)) = marker.arguments.first() else {
        return Ok(127);
    };
    let value = argument_value(context, argument).await?;
    let Value::Int(flags) = value else {
        let actual = match value {
            Value::Bool(value) => value.to_string(),
            other => other.type_name(),
        };
        return Err(NativeError::error(format!(
            "Attribute::__construct(): Argument #1 ($flags) must be of type int, {actual} given"
        )));
    };
    let flags = flags as u32;
    if flags & !255 != 0 {
        return Err(NativeError::error("Invalid attribute flags specified"));
    }
    Ok(i64::from(flags))
}

async fn argument_value(context: &mut dyn Context, argument: &AttributeValue) -> NativeResult {
    match argument {
        AttributeValue::Expression(initializer) => {
            engine(context)?.reflect_evaluate(initializer.clone()).await
        }
        AttributeValue::Literal(value) => Ok(value.clone()),
    }
}

async fn arguments(
    context: &mut dyn Context,
    attribute: &AttributeInfo,
) -> Result<PhpArray, NativeError> {
    let mut result = PhpArray::new();
    for (name, argument) in &attribute.arguments {
        let value = argument_value(context, argument).await?;
        match name {
            Some(name) => {
                result.insert(ArrayKey::str(name.as_str()), value);
            }
            None => {
                result.push(value)?;
            }
        }
    }
    Ok(result)
}

fn missing_class(error: &NativeError) -> bool {
    matches!(error, NativeError::Throw { class: "ReflectionException", message, .. }
        if message.starts_with("Class \"") && message.ends_with("\" does not exist"))
}

async fn lookup(
    context: &mut dyn Context,
    name: &str,
    missing: String,
) -> Result<phera_extension_api::reflection::ClassInfo, NativeError> {
    engine(context)?.reflect_class(name).await.map_err(|error| {
        if missing_class(&error) {
            NativeError::error(missing)
        } else {
            error
        }
    })
}
