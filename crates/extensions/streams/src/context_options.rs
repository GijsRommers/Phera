//! Stream contexts hold options, and a stream can retain its context.
//!
//! $options = stream_context_create(['file' => ['example' => true]]);
//! stream_context_get_options(fopen('file.txt', 'r', context: $options));

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args, value::Resource,
};

use crate::request::{files, new_resource};
use crate::resource::{argument_resource, given, with_stream};

pub const OPTIONAL_CONTEXT: Param = Param::new("context", ParamType::Mixed).optional();

struct Options(PhpArray);

pub fn register(registry: &mut Registry) {
    const CREATE: &[Param] = &[
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
        Param::new("params", ParamType::Array).nullable().optional(),
    ];
    const GET: &[Param] = &[Param::new("stream_or_context", ParamType::Mixed)];
    const SET: &[Param] = &[
        Param::new("context", ParamType::Mixed),
        Param::new("wrapper_or_options", ParamType::ArrayOrString),
        Param::new("option_name", ParamType::String)
            .nullable()
            .optional(),
        Param::new("value", ParamType::Mixed).optional(),
    ];
    for entry in [
        FunctionEntry::sync("stream_context_create", CREATE, create),
        FunctionEntry::sync("stream_context_get_options", GET, get_options),
        FunctionEntry::sync("stream_context_set_option", SET, set_option),
    ] {
        registry.function(entry);
    }
}

pub fn validate(value: &Value, function: &str, index: usize) -> Result<(), NativeError> {
    match value {
        Value::Null => Ok(()),
        Value::Resource(resource) if resource.kind == "stream-context" && !resource.is_closed() => {
            Ok(())
        }
        Value::Resource(_) => Err(invalid(function)),
        other => Err(NativeError::type_error(format!(
            "{function}(): Argument #{index} ($context) must be of type resource or null, {} given",
            given(other)
        ))),
    }
}

fn create(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut options = PhpArray::new();
    if let Some(array) = args::array(arguments, 0) {
        merge(&mut options, array)?;
    }
    if let Some(params) = args::array(arguments, 1)
        && let Some(input) = params.get(&ArrayKey::str("options"))
    {
        let Value::Array(array) = input else {
            return Err(NativeError::type_error("Invalid stream/context parameter"));
        };
        merge(&mut options, array)?;
    }
    Ok(create_options(context, options))
}

fn create_options(context: &mut dyn Context, options: PhpArray) -> Value {
    new_resource(context.state(), "stream-context", Options(options))
}

fn invalid(function: &str) -> NativeError {
    NativeError::type_error(format!(
        "{function}(): supplied resource is not a valid Stream-Context resource"
    ))
}

fn attached(
    context: &mut dyn Context,
    resource: &Rc<Resource>,
    function: &str,
) -> Result<Option<Rc<Resource>>, NativeError> {
    if resource.kind == "stream-context" && !resource.is_closed() {
        return Ok(Some(resource.clone()));
    }
    with_stream(resource, |_| ()).map_err(|_| invalid(function))?;
    Ok(files(context).stream_contexts.get(&resource.id).cloned())
}

fn get_options(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let function = "stream_context_get_options";
    let resource = argument_resource(
        args::value(arguments, 0),
        function,
        "#1 ($stream_or_context)",
    )?;
    let Some(resource) = attached(context, resource, function)? else {
        return Ok(Value::from(PhpArray::new()));
    };
    let data = resource.data.borrow();
    let result = data
        .as_ref()
        .and_then(|data| data.downcast_ref::<Options>())
        .ok_or_else(|| invalid(function))?;
    Ok(Value::from(result.0.clone()))
}

fn set_option(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let function = "stream_context_set_option";
    let resource = argument_resource(args::value(arguments, 0), function, "#1 ($context)")?;
    let target = match attached(context, resource, function)? {
        Some(target) => target,
        None => {
            let options = create_options(context, PhpArray::new());
            let Value::Resource(options) = options else {
                return Err(invalid(function));
            };
            files(context)
                .stream_contexts
                .insert(resource.id, options.clone());
            options
        }
    };
    let mut data = target.data.borrow_mut();
    let options = data
        .as_mut()
        .and_then(|data| data.downcast_mut::<Options>())
        .ok_or_else(|| invalid(function))?;
    if arguments.len() == 2 {
        context.deprecated(
            concat!(
                "Calling stream_context_set_option() with 2 arguments is deprecated, ",
                "use stream_context_set_options() instead"
            )
            .into(),
        );
    }
    if let Some(array) = args::array(arguments, 1) {
        if args::string(arguments, 2).is_some() {
            return Err(NativeError::value_error(concat!(
                "stream_context_set_option(): Argument #3 ($option_name) must be null ",
                "when argument #2 ($wrapper_or_options) is an array"
            )));
        }
        if args::given(arguments, 3) {
            return Err(NativeError::value_error(concat!(
                "stream_context_set_option(): Argument #4 ($value) cannot be provided ",
                "when argument #2 ($wrapper_or_options) is an array"
            )));
        }
        merge(&mut options.0, array)?;
    } else {
        let Some(name) = args::string(arguments, 2) else {
            return Err(NativeError::value_error(concat!(
                "stream_context_set_option(): Argument #3 ($option_name) cannot be null ",
                "when argument #2 ($wrapper_or_options) is a string"
            )));
        };
        if !args::given(arguments, 3) {
            return Err(NativeError::value_error(concat!(
                "stream_context_set_option(): Argument #4 ($value) must be provided ",
                "when argument #2 ($wrapper_or_options) is a string"
            )));
        }
        set(
            &mut options.0,
            ArrayKey::str(args::bytes(arguments, 1)),
            ArrayKey::str(name.as_bytes()),
            args::value(arguments, 3).clone(),
        );
    }
    Ok(Value::Bool(true))
}

fn merge(target: &mut PhpArray, source: &PhpArray) -> Result<(), NativeError> {
    for (key, entry) in source.iter() {
        let (ArrayKey::String(_), Value::Array(options)) = (&key, entry) else {
            return Err(NativeError::value_error(
                "Options should have the form [\"wrappername\"][\"optionname\"] = $value",
            ));
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

pub(crate) fn option(value: &Value, wrapper: &str, key: &str) -> Option<Value> {
    let Value::Resource(resource) = value else {
        return None;
    };
    let data = resource.data.borrow();
    let options = data.as_ref()?.downcast_ref::<Options>()?;
    let Value::Array(options) = options.0.get(&ArrayKey::str(wrapper))? else {
        return None;
    };
    options.get(&ArrayKey::str(key)).cloned()
}
