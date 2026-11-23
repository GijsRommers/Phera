//! `readdir($directory)` consumes one directory entry, including "." and "..".

use std::io;
use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, Value, args, value::Resource,
};
use rustix::fs::{Dir, Mode, OFlags};

use crate::context_options::{OPTIONAL_CONTEXT, validate};
use crate::errors::{check_path, reason, warn_false};
use crate::io_policy::IoPolicy;
use crate::mutations::{DIRECTORY, local, path_call};
use crate::request::{bytes, directory, files, new_resource, path};
use crate::resource::given;

const HANDLE: Param = Param::new("dir_handle", ParamType::Mixed).optional();

pub fn register(registry: &mut Registry) {
    const OPEN: &[Param] = &[DIRECTORY, OPTIONAL_CONTEXT];
    const SCAN: &[Param] = &[
        DIRECTORY,
        Param::new("sorting_order", ParamType::Int).with_default(ConstantValue::Int(0)),
        OPTIONAL_CONTEXT,
    ];
    for entry in [
        FunctionEntry::asynchronous("opendir", OPEN, opendir),
        FunctionEntry::asynchronous("dir", OPEN, dir),
        FunctionEntry::asynchronous("scandir", SCAN, scandir),
        FunctionEntry::asynchronous("readdir", &[HANDLE], readdir),
        FunctionEntry::asynchronous("rewinddir", &[HANDLE], rewinddir),
        FunctionEntry::asynchronous("closedir", &[HANDLE], closedir),
        FunctionEntry::sync("getcwd", &[], getcwd),
        FunctionEntry::sync("chdir", &[DIRECTORY], chdir),
    ] {
        registry.function(entry);
    }
    for (name, value) in [
        ("SCANDIR_SORT_ASCENDING", 0),
        ("SCANDIR_SORT_DESCENDING", 1),
        ("SCANDIR_SORT_NONE", 2),
    ] {
        registry.constant(name, value);
    }
}

fn opendir(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(open(context, arguments, "opendir"))
}

fn dir(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = args::value(&arguments, 0).clone();
        let value = open(context, arguments, "dir").await?;
        if !value.is_resource() {
            return Ok(value);
        }
        let mut properties = PhpArray::new();
        properties.insert(ArrayKey::str("path"), name);
        properties.insert(ArrayKey::str("handle"), value);
        context.create_object("Directory", properties)
    })
}

async fn open(context: &mut dyn Context, arguments: Vec<Value>, function: &str) -> NativeResult {
    let name = args::bytes(&arguments, 0);
    check_path(function, "#1 ($directory)", name)?;
    let stream_context = args::value(&arguments, 1);
    validate(stream_context, function, 2)?;
    files(context).use_default_context(stream_context);
    if name.is_empty() {
        return Ok(Value::Bool(false));
    }
    if let Some(wrapper) = crate::user_wrappers::locate(context, name) {
        let supplied = if stream_context.is_null() {
            crate::context_options::default_for_wrapper(context)
        } else {
            stream_context.clone()
        };
        let Some(dir) =
            crate::user_directory::open(context, wrapper, function, name, supplied).await?
        else {
            return warn_false(
                context,
                format!(
                    "{}: Failed to open directory: operation failed",
                    path_call(function, name)
                ),
            );
        };
        let value = crate::user_resource::make_directory(context, dir);
        if let Value::Resource(resource) = &value {
            files(context).directory = Some(resource.clone());
        }
        return Ok(value);
    }
    match open_directory(context, name, function).await? {
        Ok(dir) => {
            let value = new_resource(context.state(), "stream", dir);
            if let Value::Resource(resource) = &value {
                files(context).directory = Some(resource.clone());
            }
            Ok(value)
        }
        Err(error) => warn_false(
            context,
            format!(
                "{}: Failed to open directory: {}",
                path_call(function, name),
                reason(&error)
            ),
        ),
    }
}

async fn open_directory(
    context: &mut dyn Context,
    name: &[u8],
    function: &str,
) -> Result<io::Result<Dir>, NativeError> {
    let name = match local(context, function, name) {
        Ok(name) => name,
        Err(error) => return Ok(Err(error)),
    };
    IoPolicy::Blocking
        .run(move || {
            let descriptor = rustix::fs::open(
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
                Mode::empty(),
            )?;
            Dir::new(descriptor).map_err(Into::into)
        })
        .await
}

/// Reads a directory through the same wrapper and request directory as scandir().
pub async fn directory_entries(
    context: &mut dyn Context,
    name: &[u8],
) -> Result<io::Result<Vec<Vec<u8>>>, NativeError> {
    collect_entries(context, name, 2, Value::Null).await
}

async fn collect_entries(
    context: &mut dyn Context,
    name: &[u8],
    order: i64,
    stream_context: Value,
) -> Result<io::Result<Vec<Vec<u8>>>, NativeError> {
    if let Some(wrapper) = crate::user_wrappers::locate(context, name) {
        let function = "scandir";
        let Some(dir) =
            crate::user_directory::open(context, wrapper, function, name, stream_context).await?
        else {
            return Ok(Err(io::Error::other("operation failed")));
        };
        let result = async {
            let mut entries = Vec::new();
            while let Some(entry) = dir.read(context, function).await?.to_php_string() {
                entries.push(entry.as_bytes().to_vec());
            }
            sort(&mut entries, order);
            Ok::<_, NativeError>(entries)
        }
        .await;
        let closed = dir.close(context).await;
        let entries = result?;
        closed?;
        return Ok(Ok(entries));
    }
    let directory = match open_directory(context, name, "scandir").await? {
        Ok(directory) => directory,
        Err(error) => return Ok(Err(error)),
    };
    let entries = IoPolicy::Blocking
        .run(move || {
            let mut entries: Vec<_> = directory
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_bytes().to_vec())
                .collect();
            sort(&mut entries, order);
            entries
        })
        .await?;
    Ok(Ok(entries))
}

fn sort(entries: &mut [Vec<u8>], order: i64) {
    match order {
        0 => entries.sort_unstable(),
        2 => (),
        _ => {
            entries.sort_unstable();
            entries.reverse();
        }
    }
}

fn scandir(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = args::bytes(&arguments, 0);
        check_path("scandir", "#1 ($directory)", name)?;
        let stream_context = args::value(&arguments, 2);
        validate(stream_context, "scandir", 3)?;
        files(context).use_default_context(stream_context);
        if name.is_empty() {
            return Err(NativeError::value_error(
                "scandir(): Argument #1 ($directory) must not be empty",
            ));
        }
        let order = args::int(&arguments, 1).unwrap_or(0);
        let entries = match collect_entries(context, name, order, stream_context.clone()).await? {
            Ok(entries) => entries,
            Err(error) => {
                let description = reason(&error);
                context.warning(format!(
                    "{}: Failed to open directory: {description}",
                    path_call("scandir", name)
                ));
                let errno = error.raw_os_error().unwrap_or(0);
                return warn_false(
                    context,
                    format!("scandir(): (errno {errno}): {description}"),
                );
            }
        };
        let mut result = PhpArray::new();
        for entry in entries {
            result.push(Value::from(entry))?;
        }
        Ok(Value::from(result))
    })
}

fn readdir(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let handle = handle(context, &arguments, "readdir")?;
        read(context, &handle, "readdir").await
    })
}

fn rewinddir(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let handle = handle(context, &arguments, "rewinddir")?;
        rewind(context, &handle).await
    })
}

fn closedir(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let handle = handle(context, &arguments, "closedir")?;
        close(context, &handle).await
    })
}

pub async fn close(context: &mut dyn Context, resource: &Resource) -> NativeResult {
    let dir = crate::user_directory::of(resource);
    if dir.as_ref().is_some_and(|dir| dir.is_busy()) {
        return Err(NativeError::error(
            "Directory is already executing a callback",
        ));
    }
    if let Some(dir) = dir {
        crate::user_resource::release(context, resource);
        dir.close(context).await?;
    } else {
        resource.data.borrow_mut().take();
    }
    Ok(Value::Null)
}

fn handle(
    context: &mut dyn Context,
    arguments: &[Value],
    function: &str,
) -> Result<Rc<Resource>, NativeError> {
    let handle = match args::value(arguments, 0) {
        Value::Resource(resource) => resource.clone(),
        Value::Null => {
            context.deprecated(format!(
                "{}(){}",
                function,
                concat!(
                    ": Passing null is deprecated, instead the last opened ",
                    "directory stream should be provided"
                )
            ));
            files(context)
                .directory
                .clone()
                .ok_or_else(|| NativeError::type_error("No resource supplied"))?
        }
        other => {
            return Err(NativeError::type_error(format!(
                "{}(): Argument #1 ($dir_handle) must be of type resource or null, {} given",
                function,
                given(other)
            )));
        }
    };
    if handle.is_closed() {
        return Err(NativeError::type_error(format!(
            "{function}(): Argument #1 ($dir_handle) must be an open stream resource"
        )));
    }
    if !is_open(&handle) {
        return Err(NativeError::type_error(format!(
            "{function}(): Argument #1 ($dir_handle) must be a valid Directory resource"
        )));
    }
    Ok(handle)
}

pub fn is_open(resource: &Resource) -> bool {
    resource.data.borrow().as_ref().is_some_and(|dir| {
        dir.is::<Dir>()
            || dir.is::<crate::user_resource::UserHandle<crate::user_directory::UserDirectory>>()
    })
}

pub async fn read(context: &mut dyn Context, resource: &Resource, function: &str) -> NativeResult {
    if let Some(dir) = crate::user_directory::of(resource) {
        return dir.read(context, function).await;
    }
    with_directory(resource, |dir| {
        dir.read()
            .and_then(Result::ok)
            .map(|entry| entry.file_name().to_bytes().to_vec())
    })
    .await
    .map(|value| value.map_or(Value::Bool(false), Value::from))
}

pub async fn rewind(context: &mut dyn Context, resource: &Resource) -> NativeResult {
    if let Some(dir) = crate::user_directory::of(resource) {
        return dir.rewind(context).await;
    }
    with_directory(resource, Dir::rewind).await?;
    Ok(Value::Null)
}

async fn with_directory<T: Send + 'static>(
    resource: &Resource,
    work: impl FnOnce(&mut Dir) -> T + Send + 'static,
) -> Result<T, NativeError> {
    let mut dir = resource
        .data
        .borrow_mut()
        .take()
        .and_then(|data| data.downcast::<Dir>().ok())
        .ok_or_else(|| NativeError::error("Directory state is unavailable"))?;
    let (result, dir) = IoPolicy::Blocking
        .run(move || (work(&mut dir), dir))
        .await?;
    *resource.data.borrow_mut() = Some(dir);
    Ok(result)
}

fn getcwd(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(directory(context)
        .path()
        .map_or(Value::Bool(false), |cwd| Value::from(bytes(cwd).to_vec())))
}

fn chdir(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let name = args::bytes(arguments, 0);
    check_path("chdir", "#1 ($directory)", name)?;
    let resolved = if name.is_empty() {
        Err(rustix::io::Errno::NOENT.into())
    } else {
        directory(context).resolve(path(name))
    };
    let expected = resolved.and_then(|name| {
        use rustix::fs::{Access, access};
        let name = name.canonicalize()?;
        let result = name.metadata()?;
        if !result.is_dir() {
            return Err(rustix::io::Errno::NOTDIR.into());
        }
        access(&name, Access::EXEC_OK)?;
        Ok(name)
    });
    match expected {
        Ok(cwd) => {
            directory(context).set(cwd);
            Ok(Value::Bool(true))
        }
        Err(error) => warn_false(
            context,
            format!(
                "chdir(): {} (errno {})",
                reason(&error),
                error.raw_os_error().unwrap_or(0)
            ),
        ),
    }
}
