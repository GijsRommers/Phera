//! `stream_filter_remove($filter)` flushes that filter before detaching it.

use std::rc::{Rc, Weak};

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, Value, args, value::Resource,
};

use crate::handles::STREAM;
use crate::request::new_resource;
use crate::resource::{argument_resource, stream_arg, with_stream};
use crate::stream::Wrapper;

use super::{Filter, Filters, MAX_STAGES};

pub(super) const BUILTINS: &[&str] = &[
    "zlib.*",
    "convert.iconv.*",
    "string.rot13",
    "string.toupper",
    "string.tolower",
    "convert.*",
    "consumed",
    "dechunk",
];

const NAME: Param = Param::new("filter_name", ParamType::String);
const INSERT: &[Param] = &[
    STREAM,
    NAME,
    Param::new("mode", ParamType::Int).with_default(ConstantValue::Int(0)),
    Param::new("params", ParamType::Mixed).optional(),
];

pub fn register(registry: &mut Registry) {
    super::user::register(registry);
    for function in [
        FunctionEntry::sync("stream_get_filters", &[], get_filters),
        FunctionEntry::asynchronous("stream_filter_append", INSERT, append),
        FunctionEntry::asynchronous("stream_filter_prepend", INSERT, prepend),
        FunctionEntry::asynchronous(
            "stream_filter_remove",
            const { &[Param::new("stream_filter", ParamType::Mixed)] },
            remove,
        ),
    ] {
        registry.function(function);
    }
    for (name, value) in [
        ("STREAM_FILTER_READ", 1),
        ("STREAM_FILTER_WRITE", 2),
        ("STREAM_FILTER_ALL", 3),
    ] {
        registry.constant(name, value);
    }
}

struct Attachment(Weak<Resource>);

fn get_filters(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    let mut names = PhpArray::new();
    for &name in BUILTINS {
        names.push(Value::from(name))?;
    }
    super::user::names(context, &mut names)?;
    Ok(Value::from(names))
}

fn append(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(attach(context, arguments, "stream_filter_append", false))
}

fn prepend(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(attach(context, arguments, "stream_filter_prepend", true))
}

async fn attach(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
    prepend: bool,
) -> NativeResult {
    let stream = stream_arg(&arguments, function)?;
    let name = args::bytes(&arguments, 1);
    let mode = args::int(&arguments, 2).unwrap_or_default();
    let params = args::value(&arguments, 3);
    let user_stream = crate::user_file::resource_file(stream);
    let direction = |flags: &[u8]| {
        if mode == 0 {
            (
                flags.contains(&b'r') || flags.contains(&b'+'),
                flags.first() != Some(&b'r') || flags.contains(&b'+'),
            )
        } else {
            (mode & 1 != 0, mode & 2 != 0)
        }
    };
    let ((read, write), stages) = if let Some(file) = &user_stream {
        (direction(&file.mode), 0)
    } else {
        with_stream(stream, |stream| {
            (
                direction(&stream.mode),
                stream
                    .filters
                    .as_ref()
                    .map_or(0, |filters| filters.read.len() + filters.write.len()),
            )
        })?
    };
    if let Some(class) = super::user::class(context, name) {
        return super::user::attach(
            context,
            stream,
            function,
            name,
            &class,
            params,
            ((read, write), prepend),
        )
        .await;
    }
    if user_stream.is_some() {
        context.warning(format!(
            "{function}(): Built-in filters on user wrappers are unsupported"
        ));
        return Ok(Value::Bool(false));
    }
    if stages + usize::from(read) + usize::from(write) > MAX_STAGES {
        context.warning(format!(
            "{function}(): Stream filter chain exceeds Phera's limit of 16 filters"
        ));
        return Ok(Value::Bool(false));
    }
    let mut read_filter = read.then(|| Filter::new(0, name, params)).flatten();
    let mut write_filter = write.then(|| Filter::new(0, name, params)).flatten();
    if read && read_filter.is_none() || write && write_filter.is_none() {
        let name = String::from_utf8_lossy(name);
        context.warning(format!("{function}(): Unable to locate filter \"{name}\""));
        return Ok(Value::Bool(false));
    }
    if !read && !write {
        return Ok(Value::Bool(false));
    }
    let value = new_resource(
        context.state(),
        "stream filter",
        Attachment(Rc::downgrade(stream)),
    );
    let Value::Resource(filter) = &value else {
        return Err(NativeError::error(
            "Stream filter resource allocation failed",
        ));
    };
    with_stream(stream, |stream| {
        let filters = stream
            .filters
            .get_or_insert_with(|| Box::new(Filters::default()));
        if let Some(mut stage) = read_filter.take() {
            stage.id = filter.id;
            filters.read.add(stage, prepend);
            filters.read_closed = false;
        }
        if let Some(mut stage) = write_filter.take() {
            stage.id = filter.id;
            filters.write.add(stage, prepend);
        }
    })?;
    Ok(value)
}

fn remove(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let resource = argument_resource(
            args::value(&arguments, 0),
            "stream_filter_remove",
            "#1 ($stream_filter)",
        )?;
        if let Some(result) = super::user::remove(context, resource).await? {
            return Ok(result);
        }
        let stream = resource
            .data
            .borrow()
            .as_ref()
            .and_then(|value| value.downcast_ref::<Attachment>())
            .and_then(|attachment| attachment.0.upgrade())
            .filter(|stream| !stream.is_closed());
        let Some(stream) = stream else {
            return Err(NativeError::type_error(
                "stream_filter_remove(): supplied resource is not a valid stream filter resource",
            ));
        };
        let result = crate::resource::operate(&stream, crate::io_policy::IoPolicy::Blocking, {
            let id = resource.id;
            move |stream| {
                let wrapper = stream.wrapper;
                let Some(filters) = &mut stream.filters else {
                    return Ok((wrapper, Vec::new()));
                };
                if let Some(buffer) = filters.read.remove(id)? {
                    super::bounded(filters.buffered.len() + buffer.len())?;
                    filters.buffered.extend(buffer);
                }
                let pending = filters.write.remove(id)?.unwrap_or_default();
                stream.write_filter_output(&pending)?;
                Ok::<_, std::io::Error>((wrapper, pending))
            }
        })
        .await?;
        match result {
            Ok((wrapper, output)) => {
                if wrapper == Wrapper::Output {
                    context.write(&output);
                }
                resource.data.borrow_mut().take();
                Ok(Value::Bool(true))
            }
            Err(error) => {
                context.warning(format!("stream_filter_remove(): {error}"));
                Ok(Value::Bool(false))
            }
        }
    })
}
