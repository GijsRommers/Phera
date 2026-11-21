//! `get_resource_type($file)` reports "stream", and "Unknown" after closing it.

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::{Context, NativeError, NativeResult, Registry, Value, args};
use phera_extension_api::{FunctionEntry, Param, ParamType, value::Resource};

use crate::io_policy::IoPolicy;
use crate::stream::Stream;
use crate::user_resource::UserHandle;

pub(crate) type FilteredBacking = Rc<RefCell<Option<Box<Stream>>>>;
pub(crate) type FilteredStream = UserHandle<RefCell<Option<Box<Stream>>>>;

pub fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[Param::new("resource", ParamType::Mixed)];
    registry.function(FunctionEntry::sync("get_resource_type", PARAMS, get_type));
    registry.function(FunctionEntry::sync("get_resource_id", PARAMS, get_id));
}

pub fn given(value: &Value) -> String {
    match value {
        Value::Bool(true) => "true".into(),
        Value::Bool(false) => "false".into(),
        other => other.type_name(),
    }
}

pub fn argument_resource<'a>(
    value: &'a Value,
    function: &str,
    argument: &str,
) -> Result<&'a Rc<Resource>, NativeError> {
    match value {
        Value::Resource(resource) => Ok(resource),
        other => Err(NativeError::type_error(format!(
            "{function}(): Argument {argument} must be of type resource, {} given",
            given(other)
        ))),
    }
}

fn get_type(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let resource = argument_resource(args::value(args, 0), "get_resource_type", "#1 ($resource)")?;
    Ok(Value::from(if resource.is_closed() {
        "Unknown"
    } else {
        resource.kind
    }))
}

fn get_id(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let resource = argument_resource(args::value(args, 0), "get_resource_id", "#1 ($resource)")?;
    Ok(Value::Int(resource.id))
}

pub fn stream_argument<'a>(
    value: &'a Value,
    function: &str,
    argument: &str,
) -> Result<&'a Rc<Resource>, NativeError> {
    let resource = argument_resource(value, function, argument)?;
    if resource.is_closed() || resource.kind != "stream" {
        return Err(NativeError::type_error(format!(
            "{function}(): Argument {argument} must be an open stream resource"
        )));
    }
    // Directory handles share the PHP label, but have different operations.
    let valid = resource.data.borrow().as_ref().is_some_and(|data| {
        data.is::<Stream>()
            || data.is::<FilteredStream>()
            || data.is::<crate::user_resource::UserHandle<crate::user_file::UserFile>>()
    });
    if !valid {
        return Err(NativeError::type_error(format!(
            "{function}(): Argument {argument} must be an open stream resource"
        )));
    }
    Ok(resource)
}

pub fn stream_arg<'a>(args: &'a [Value], function: &str) -> Result<&'a Rc<Resource>, NativeError> {
    stream_argument(args::value(args, 0), function, "#1 ($stream)")
}

pub fn with_stream<T>(
    resource: &Resource,
    work: impl FnOnce(&mut Stream) -> T,
) -> Result<T, NativeError> {
    let mut slot = resource.data.borrow_mut();
    if let Some(stream) = slot
        .as_mut()
        .and_then(|stream| stream.downcast_mut::<Stream>())
    {
        return Ok(work(stream));
    }
    let filtered = slot
        .as_ref()
        .and_then(|stream| stream.downcast_ref::<FilteredStream>())
        .ok_or_else(unavailable)?;
    let mut backing = filtered.value.borrow_mut();
    let stream = backing.as_deref_mut().ok_or_else(unavailable)?;
    Ok(work(stream))
}

/// Own the Rust stream across the await and restore it even after an I/O error.
/// The PHP resource and its other aliases stay on the request thread.
pub async fn operate<T: Send + 'static>(
    resource: &Resource,
    policy: IoPolicy,
    work: impl FnOnce(&mut Stream) -> T + Send + 'static,
) -> Result<T, NativeError> {
    let data = resource.data.borrow_mut().take().ok_or_else(unavailable)?;
    let (mut stream, filtered) = match data.downcast::<Stream>() {
        Ok(stream) => (stream, None),
        Err(data) => {
            let filtered = data
                .downcast::<FilteredStream>()
                .map_err(|_| unavailable())?;
            let stream = filtered.value.borrow_mut().take().ok_or_else(unavailable)?;
            (stream, Some(filtered))
        }
    };
    let (result, stream) = policy
        .run(move || {
            let result = work(&mut stream);
            (result, stream)
        })
        .await?;
    *resource.data.borrow_mut() = Some(match filtered {
        Some(filtered) => {
            *filtered.value.borrow_mut() = Some(stream);
            filtered
        }
        None => stream,
    });
    Ok(result)
}

/// Keep the backing alive while the last PHP owner queues the filter close.
pub(crate) fn retain_filtered(
    context: &dyn Context,
    resource: &Resource,
) -> Option<FilteredBacking> {
    let mut slot = resource.data.borrow_mut();
    if !slot.as_ref().is_some_and(|data| data.is::<Stream>()) {
        return None;
    }
    let stream = slot.take()?.downcast::<Stream>().ok()?;
    let backing = Rc::new(RefCell::new(Some(stream)));
    let release = context.resource_release("stream", resource.id);
    *slot = Some(Box::new(FilteredStream::new(backing.clone(), release)));
    Some(backing)
}

pub(crate) fn restore_unfiltered(resource: &Resource) {
    let mut slot = resource.data.borrow_mut();
    if let Some(filtered) = slot
        .as_mut()
        .and_then(|data| data.downcast_mut::<FilteredStream>())
    {
        filtered.cancel_release();
        let stream = filtered.value.borrow_mut().take();
        *slot = stream.map(|stream| stream as Box<dyn std::any::Any>);
    }
}

pub(crate) fn close_backing(resource: &Resource) {
    let mut slot = resource.data.borrow_mut();
    if let Some(filtered) = slot
        .as_mut()
        .and_then(|data| data.downcast_mut::<FilteredStream>())
    {
        filtered.cancel_release();
    }
    slot.take();
}

fn unavailable() -> NativeError {
    NativeError::error("Stream state is unavailable")
}
