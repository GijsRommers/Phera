//! Stream contexts carry options independently of the streams that use them.
//!
//! `stream_context_create(['file' => ['example' => true]])` keeps the option.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

use crate::request::new_resource;
use crate::resource::{argument_resource, given};

pub const OPTIONAL_CONTEXT: Param = Param::new("context", ParamType::Mixed).optional();

struct Options(PhpArray);

pub fn register(registry: &mut Registry) {
    registry.function(
        const {
            FunctionEntry::sync(
                "stream_context_create",
                const {
                    &[
                        Param::new("options", ParamType::Array)
                            .nullable()
                            .optional(),
                        Param::new("params", ParamType::Array).nullable().optional(),
                    ]
                },
                create,
            )
        },
    );
    registry.function(
        const {
            FunctionEntry::sync(
                "stream_context_get_options",
                const { &[Param::new("stream_or_context", ParamType::Mixed)] },
                get_options,
            )
        },
    );
    registry.function(
        const {
            FunctionEntry::sync(
                "stream_context_set_option",
                const {
                    &[
                        Param::new("context", ParamType::Mixed),
                        Param::new("wrapper_or_options", ParamType::ArrayOrString),
                        Param::new("option_name", ParamType::String)
                            .nullable()
                            .optional(),
                        Param::new("value", ParamType::Mixed).optional(),
                    ]
                },
                set_option,
            )
        },
    );
}

pub fn validate(value: &Value, function: &str, index: usize) -> Result<(), NativeError> {
    match value {
        Value::Null => Ok(()),
        Value::Resource(resource) if resource.kind == "stream-context" && !resource.is_closed() => {
            Ok(())
        }
        Value::Resource(_) => Err(NativeError::type_error(format!(
            "{function}(): supplied resource is not a valid Stream-Context resource"
        ))),
        other => Err(NativeError::type_error(format!(
            "{function}(): Argument #{index} ($context) must be of type resource or null, {} given",
            given(other)
        ))),
    }
}

fn create(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut options = PhpArray::new();
    if let Some(array) = args::array(arguments, 0) {
        merge(&mut options, array, "stream_context_create")?;
    }
    Ok(new_resource(
        context.state(),
        "stream-context",
        Options(options),
    ))
}

fn get_options(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let function = "stream_context_get_options";
    let resource = argument_resource(
        args::value(arguments, 0),
        function,
        "#1 ($stream_or_context)",
    )?;
    let data = resource.data.borrow();
    if let Some(options) = data
        .as_ref()
        .and_then(|data| data.downcast_ref::<Options>())
    {
        return Ok(Value::from(options.0.clone()));
    }
    if data
        .as_ref()
        .is_some_and(|data| data.is::<crate::stream::Stream>())
    {
        return Ok(Value::from(PhpArray::new()));
    }
    Err(NativeError::type_error(format!(
        "{function}(): supplied resource is not a valid Stream-Context resource"
    )))
}

fn set_option(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let function = "stream_context_set_option";
    let resource = argument_resource(args::value(arguments, 0), function, "#1 ($context)")?;
    let mut data = resource.data.borrow_mut();
    let options = data
        .as_mut()
        .and_then(|data| data.downcast_mut::<Options>())
        .ok_or_else(|| {
            NativeError::type_error(format!(
                "{function}(): supplied resource is not a valid Stream-Context resource"
            ))
        })?;
    if let Some(array) = args::array(arguments, 1) {
        context.deprecated(
            "Calling stream_context_set_option() with 2 arguments is deprecated,              use stream_context_set_options() instead".into(),
        );
        merge(&mut options.0, array, function)?;
    } else {
        let Some(name) = args::string(arguments, 2) else {
            return Err(NativeError::type_error(
                "stream_context_set_option(): Argument #3 ($option_name) must be of type string,                  null given when argument #2 ($wrapper_or_options) is a string",
            ));
        };
        set(
            &mut options.0,
            ArrayKey::str(args::bytes(arguments, 1)),
            ArrayKey::str(name.as_bytes()),
            args::value(arguments, 3).clone(),
        );
    }
    Ok(Value::Bool(true))
}

fn merge(target: &mut PhpArray, source: &PhpArray, function: &str) -> Result<(), NativeError> {
    for (key, entry) in source.iter() {
        let (ArrayKey::String(_), Value::Array(options)) = (&key, entry) else {
            return Err(NativeError::value_error(format!(
                "{function}(): Argument #1 ($options) must have the form                  [\"wrapper\" => [\"option\" => $value]]"
            )));
        };
        for (option, value) in options.iter() {
            if let ArrayKey::String(_) = option {
                set(target, key.clone(), option.clone(), value.clone());
            }
        }
    }
    Ok(())
}

fn set(target: &mut PhpArray, wrapper: ArrayKey, option: ArrayKey, value: Value) {
    let mut options = match target.get(&wrapper) {
        Some(Value::Array(options)) => (**options).clone(),
        _ => PhpArray::new(),
    };
    options.insert(option, value);
    target.insert(wrapper, Value::from(options));
}
