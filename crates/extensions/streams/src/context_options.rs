//! Stream contexts hold options, and a stream can retain its context.
//!
//! $options = stream_context_create(['file' => ['example' => true]]);
//! stream_context_get_options(fopen('file.txt', 'r', context: $options));

mod parameters;

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args, value::Resource,
};

use self::parameters::{get_params, set_params, update_params};
use crate::request::{files, new_resource};
use crate::resource::{argument_resource, given, with_stream};

pub const OPTIONAL_CONTEXT: Param = Param::new("context", ParamType::Mixed).optional();

#[derive(Clone, Default)]
struct Options {
    wrappers: PhpArray,
    notification: Option<Value>,
}

pub fn register(registry: &mut Registry) {
    const CREATE: &[Param] = &[
        Param::new("options", ParamType::Array)
            .nullable()
            .optional(),
        Param::new("params", ParamType::Array).nullable().optional(),
    ];
    const GET: &[Param] = &[Param::new("stream_or_context", ParamType::Mixed)];
    const CONTEXT: Param = Param::new("context", ParamType::Mixed);
    const OPTIONS: Param = Param::new("options", ParamType::Array);
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
        FunctionEntry::sync(
            "stream_context_get_default",
            const { &[OPTIONS.nullable().optional()] },
            get_default,
        ),
        FunctionEntry::sync(
            "stream_context_set_default",
            const { &[OPTIONS] },
            set_default,
        ),
        FunctionEntry::sync(
            "stream_context_get_params",
            const { &[CONTEXT] },
            get_params,
        ),
        FunctionEntry::sync(
            "stream_context_set_params",
            const { &[CONTEXT, Param::new("params", ParamType::Array)] },
            set_params,
        ),
        FunctionEntry::sync(
            "stream_context_set_options",
            const { &[CONTEXT, OPTIONS] },
            set_options,
        ),
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
    let mut options = Options::default();
    if let Some(array) = args::array(arguments, 0) {
        merge(&mut options.wrappers, array)?;
    }
    if let Some(params) = args::array(arguments, 1) {
        update_params(
            context,
            &mut options,
            params,
            "stream_context_create",
            "options",
        )?;
    }
    Ok(new_resource(context.state(), "stream-context", options))
}

fn get_default(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let value = default_for_wrapper(context);
    if let Some(options) = args::array(arguments, 0) {
        merge_into(&value, options, "stream_context_get_default")?;
    }
    Ok(value)
}

fn set_default(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let value = default_for_wrapper(context);
    if let Some(options) = args::array(arguments, 0) {
        merge_into(&value, options, "stream_context_set_default")?;
    }
    Ok(value)
}

pub(crate) fn default_for_wrapper(context: &mut dyn Context) -> Value {
    let files = files(context);
    files.use_default_context(&Value::Null);
    if files.default_context.is_none()
        && let Some(id) = files.default_context_id
    {
        files.default_context = Some(crate::request::resource(
            id,
            "stream-context",
            Options::default(),
        ));
    }
    files.default_context.clone().unwrap_or(Value::Null)
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
    parameter: &str,
) -> Result<Option<Rc<Resource>>, NativeError> {
    if resource.kind == "stream-context" && !resource.is_closed() {
        return Ok(Some(resource.clone()));
    }
    if let Some(file) = crate::user_file::resource_file(resource) {
        return Ok(match &file.context {
            Value::Resource(resource) => Some(resource.clone()),
            _ => None,
        });
    }
    if !crate::directory::is_open(resource)
        && crate::user_directory::of(resource).is_none()
        && with_stream(resource, |_| ()).is_err()
    {
        return Err(invalid_target(function, parameter));
    }
    Ok(files(context).stream_contexts.get(&resource.id).cloned())
}

fn invalid_target(function: &str, parameter: &str) -> NativeError {
    NativeError::type_error(format!(
        "{function}(): Argument #1 (${parameter}) must be a valid stream/context"
    ))
}

fn get_options(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let function = "stream_context_get_options";
    let options = read_options(context, arguments, function, "stream_or_context")?;
    Ok(Value::from(options.wrappers))
}

fn read_options(
    context: &mut dyn Context,
    arguments: &[Value],
    function: &str,
    parameter: &str,
) -> Result<Options, NativeError> {
    let resource = argument_resource(
        args::value(arguments, 0),
        function,
        &format!("#1 (${parameter})"),
    )?;
    let Some(target) = attached(context, resource, function, parameter)? else {
        return Ok(Options::default());
    };
    let data = target.data.borrow();
    Ok(data
        .as_ref()
        .and_then(|data| data.downcast_ref::<Options>())
        .ok_or_else(|| invalid_target(function, parameter))?
        .clone())
}

fn writable_context(
    context: &mut dyn Context,
    value: &Value,
    function: &str,
) -> Result<Rc<Resource>, NativeError> {
    let resource = argument_resource(value, function, "#1 ($context)")?;
    if let Some(target) = attached(context, resource, function, "context")? {
        return Ok(target);
    }
    let Value::Resource(target) =
        new_resource(context.state(), "stream-context", Options::default())
    else {
        return Err(invalid_target(function, "context"));
    };
    files(context)
        .stream_contexts
        .insert(resource.id, target.clone());
    Ok(target)
}

pub(crate) fn attach(context: &mut dyn Context, stream: &Value, options: &Value) {
    if let (Value::Resource(stream), Value::Resource(options)) = (stream, &*options.deref()) {
        files(context)
            .stream_contexts
            .insert(stream.id, options.clone());
    }
}

fn update_options<T>(
    target: &Rc<Resource>,
    function: &str,
    update: impl FnOnce(&mut Options) -> Result<T, NativeError>,
) -> Result<T, NativeError> {
    let mut data = target.data.borrow_mut();
    let options = data
        .as_mut()
        .and_then(|data| data.downcast_mut::<Options>())
        .ok_or_else(|| invalid_target(function, "context"))?;
    update(options)
}

fn merge_into(target: &Value, options: &PhpArray, function: &str) -> Result<(), NativeError> {
    let Value::Resource(target) = target else {
        return Err(invalid_target(function, "context"));
    };
    update_options(target, function, |data| merge(&mut data.wrappers, options))
}

fn set_options(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let function = "stream_context_set_options";
    let target = writable_context(context, args::value(arguments, 0), function)?;
    if let Some(array) = args::array(arguments, 1) {
        merge_into(&Value::Resource(target), array, function)?;
    }
    Ok(Value::Bool(true))
}

fn set_option(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let function = "stream_context_set_option";
    let target = writable_context(context, args::value(arguments, 0), function)?;
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
        merge_into(&Value::Resource(target), array, function)?;
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
        update_options(&target, function, |options| {
            set(
                &mut options.wrappers,
                ArrayKey::str(args::bytes(arguments, 1)),
                ArrayKey::str(name.as_bytes()),
                args::value(arguments, 3).clone(),
            );
            Ok(())
        })?;
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
    let Value::Array(options) = options.wrappers.get(&ArrayKey::str(wrapper))? else {
        return None;
    };
    options.get(&ArrayKey::str(key)).cloned()
}

/// A fork gets its own options and a copy of the notification callable.
pub(crate) fn fork_context(
    data: &dyn std::any::Any,
    fork: &mut phera_extension_api::value::Fork<'_>,
) -> Result<Box<dyn std::any::Any>, String> {
    let options = data
        .downcast_ref::<Options>()
        .ok_or("an unknown stream context")?;
    let mut copy = options.clone();
    for (_, value) in copy.wrappers.iter_mut() {
        *value = fork.value(value);
    }
    copy.notification = options.notification.as_ref().map(|value| fork.value(value));
    Ok(Box::new(copy))
}

pub(crate) fn for_wrapper(context: &Value, wrapper: &str) -> PhpArray {
    let Value::Resource(resource) = &*context.deref() else {
        return PhpArray::new();
    };
    let data = resource.data.borrow();
    let options = data
        .as_ref()
        .and_then(|data| data.downcast_ref::<Options>());
    match options.and_then(|options| options.wrappers.get(&ArrayKey::str(wrapper))) {
        Some(Value::Array(options)) => options.as_ref().clone(),
        _ => PhpArray::new(),
    }
}
