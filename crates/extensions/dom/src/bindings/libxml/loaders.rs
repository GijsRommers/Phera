//! Remember legacy entity-loader configuration without consulting it during parsing.
//!
//! ```php
//! libxml_set_external_entity_loader($resolver);
//! $document->loadXML($source, LIBXML_NOENT); // never calls $resolver
//! ```

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry,
    Value, args,
};

#[derive(Default)]
struct Config {
    resolver: Option<Value>,
    stream_context: Option<Value>,
    disabled: bool,
}

pub(super) fn register(registry: &mut Registry) {
    registry.fork_state::<Config>(fork_config);
    registry.function(FunctionEntry::sync(
        "libxml_set_external_entity_loader",
        const { &[Param::new("resolver_function", ParamType::Callable).nullable()] },
        set_resolver,
    ));
    registry.function(FunctionEntry::sync(
        "libxml_get_external_entity_loader",
        &[],
        get_resolver,
    ));
    registry.function(FunctionEntry::sync(
        "libxml_set_streams_context",
        const { &[Param::new("context", ParamType::Mixed)] },
        stream_context,
    ));
    registry.function(FunctionEntry::sync(
        "libxml_disable_entity_loader",
        const { &[Param::new("disable", ParamType::Bool).with_default(ConstantValue::Bool(true))] },
        disable,
    ));
}

fn set_resolver(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    let resolver = args.first().filter(|value| !value.is_null()).cloned();
    context.state().get_or_default::<Config>().resolver = resolver;
    Ok(Value::Bool(true))
}

fn get_resolver(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(context
        .state()
        .get_or_default::<Config>()
        .resolver
        .clone()
        .unwrap_or(Value::Null))
}

fn disable(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    context.deprecated(
        concat!(
            "Function libxml_disable_entity_loader() is deprecated since 8.0, ",
            "as external entity loading is disabled by default"
        )
        .into(),
    );
    let disabled = &mut context.state().get_or_default::<Config>().disabled;
    Ok(Value::Bool(std::mem::replace(
        disabled,
        phera_extension_api::args::bool(args, 0).unwrap_or(true),
    )))
}

fn stream_context(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let argument = args::value(arguments, 0);
    match argument {
        Value::Resource(resource) if resource.kind == "stream-context" && !resource.is_closed() => {
            context.state().get_or_default::<Config>().stream_context = Some(argument.clone());
            Ok(Value::Null)
        }
        Value::Resource(_) => Err(NativeError::type_error(concat!(
            "libxml_set_streams_context(): supplied resource ",
            "is not a valid Stream-Context resource"
        ))),
        value => Err(NativeError::type_error(format!(
            concat!(
                "libxml_set_streams_context(): Argument #1 ($context) ",
                "must be of type resource, {} given"
            ),
            value.type_name(),
        ))),
    }
}

/// The entity loader a boot set stays set in each fork, on the fork's copy.
fn fork_config(
    state: &dyn std::any::Any,
    fork: &mut phera_extension_api::value::Fork<'_>,
) -> Option<Box<dyn std::any::Any>> {
    let config = state.downcast_ref::<Config>()?;
    Some(Box::new(Config {
        resolver: config.resolver.as_ref().map(|value| fork.value(value)),
        stream_context: config
            .stream_context
            .as_ref()
            .map(|value| fork.value(value)),
        disabled: config.disabled,
    }))
}
