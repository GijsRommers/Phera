//! `readdir($directory)` consumes one directory entry, including "." and "..".

use std::io;
use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassKind, ConstantValue, Context, FunctionEntry, LocalFuture,
    MethodEntry, NativeError, NativeResult, Object, Param, ParamType, PhpArray, PropertyEntry,
    Registry, Value, Visibility, args, value::Resource,
};
use rustix::fs::{Dir, Mode, OFlags};

use crate::context_options::{OPTIONAL_CONTEXT, validate};
use crate::errors::{check_path, reason, warn_false};
use crate::io_policy::IoPolicy;
use crate::mutations::{DIRECTORY, local, path_call};
use crate::request::{bytes, directory, files, new_resource};
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
        FunctionEntry::sync("closedir", &[HANDLE], closedir),
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
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        properties: const {
            &[
                PropertyEntry::new("path", ConstantValue::Null, Visibility::Public).readonly(),
                PropertyEntry::new("handle", ConstantValue::Null, Visibility::Public).readonly(),
            ]
        },
        methods: const {
            &[
                MethodEntry::public("__construct", &[], construct),
                MethodEntry::public_async("read", &[], object_read),
                MethodEntry::public_async("rewind", &[], object_rewind),
                MethodEntry::public("close", &[], object_close),
            ]
        },
        ..ClassEntry::class("Directory")
    });
}

fn construct(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Err(NativeError::error(
        "Cannot directly construct Directory, use dir() instead",
    ))
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
    validate(args::value(&arguments, 1), function, 2)?;
    if name.is_empty() {
        return Ok(Value::Bool(false));
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

fn scandir(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = args::bytes(&arguments, 0);
        check_path("scandir", "#1 ($directory)", name)?;
        validate(args::value(&arguments, 2), "scandir", 3)?;
        if name.is_empty() {
            return Err(NativeError::value_error(
                "scandir(): Argument #1 ($directory) must not be empty",
            ));
        }
        let directory = match open_directory(context, name, "scandir").await? {
            Ok(dir) => dir,
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
        let order = args::int(&arguments, 1).unwrap_or(0);
        let entries = IoPolicy::Blocking
            .run(move || {
                let entries =
                    directory.map(|entry| entry.map(|e| e.file_name().to_bytes().to_vec()));
                let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
                match order {
                    0 => entries.sort_unstable(),
                    2 => (),
                    _ => {
                        entries.sort_unstable();
                        entries.reverse();
                    }
                }
                entries
            })
            .await?;
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
        read(&handle).await
    })
}

fn rewinddir(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let handle = handle(context, &arguments, "rewinddir")?;
        rewind(&handle).await
    })
}

fn closedir(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let handle = handle(context, arguments, "closedir")?;
    handle.data.borrow_mut().take();
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
                "{function}(): Passing null is deprecated, instead the last opened                  directory stream should be provided"
            ));
            files(context)
                .directory
                .clone()
                .ok_or_else(|| NativeError::type_error("No resource supplied"))?
        }
        other => {
            return Err(NativeError::type_error(format!(
                "{function}(): Argument #1 ($dir_handle) must be of type resource or null, {} given",
                given(other)
            )));
        }
    };
    if !is_open(&handle) {
        return Err(NativeError::type_error(format!(
            "{function}(): Argument #1 ($dir_handle) must be a valid Directory resource"
        )));
    }
    Ok(handle)
}

fn object_handle(object: Option<&Rc<Object>>, action: &str) -> Result<Rc<Resource>, NativeError> {
    if let Some(Value::Resource(handle)) = object.and_then(|o| o.get(&ArrayKey::str("handle")))
        && is_open(&handle)
    {
        return Ok(handle);
    }
    Err(NativeError::type_error(format!(
        "Directory::{action}(): cannot use Directory resource after it has been closed"
    )))
}

fn is_open(resource: &Resource) -> bool {
    resource
        .data
        .borrow()
        .as_ref()
        .is_some_and(|dir| dir.is::<Dir>())
}

fn object_read(
    _: &mut dyn Context,
    object: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { read(object_handle(object.as_ref(), "read")?.as_ref()).await })
}

fn object_rewind(
    _: &mut dyn Context,
    object: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { rewind(object_handle(object.as_ref(), "rewind")?.as_ref()).await })
}

fn object_close(_: &mut dyn Context, object: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    object_handle(object, "close")?.data.borrow_mut().take();
    Ok(Value::Null)
}

async fn read(resource: &Resource) -> NativeResult {
    with_directory(resource, |dir| {
        dir.read()
            .and_then(Result::ok)
            .map(|entry| entry.file_name().to_bytes().to_vec())
    })
    .await
    .map(|value| value.map_or(Value::Bool(false), Value::from))
}

async fn rewind(resource: &Resource) -> NativeResult {
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
    let expected = local(context, "chdir", name).and_then(|name| {
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
