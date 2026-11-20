//! `finfo_open(FILEINFO_MIME_TYPE)` and `new finfo(FILEINFO_MIME_TYPE)` keep flags per object.

use std::cell::Cell;
use std::rc::Rc;

use phera_ext_streams::native::{
    StreamContents, inspect_path, inspect_resource, validate_stream_context,
};
use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue as C, Context, FunctionEntry as F, LocalFuture,
    MethodEntry as M, NativeError, NativeResult, Object, Param as P, ParamType as T, PhpArray,
    Registry, Value, args,
};

use crate::classifier::{self, Format};

const CLASS: &str = "finfo";
const FINFO: P = P::new("finfo", T::Class(CLASS));
const FLAGS: P = P::new("flags", T::Int);
const DEFAULT_FLAGS: P = FLAGS.with_constant_default("FILEINFO_NONE", C::Int(0));
const DATABASE: P = P::new("magic_database", T::String)
    .without_nul()
    .nullable()
    .with_default(C::Null);
const FILENAME: P = P::new("filename", T::String).without_nul();
const BUFFER: P = P::new("string", T::String);
const CONTEXT: P = P::new("context", T::Untyped).with_default(C::Null);
const LIMIT: u64 = 7 * 1024 * 1024;

pub fn register(registry: &mut Registry) {
    for &(name, value) in classifier::FLAGS {
        registry.constant(name, value);
    }
    registry.class(ClassEntry {
        methods: &[
            M::public_async("__construct", &[DEFAULT_FLAGS, DATABASE], construct),
            M::public_async("file", &[FILENAME, DEFAULT_FLAGS, CONTEXT], method_file)
                .tentative_return("string|false"),
            M::public("buffer", &[BUFFER, DEFAULT_FLAGS, CONTEXT], method_buffer)
                .tentative_return("string|false"),
            M::public("set_flags", &[FLAGS], method_set_flags).tentative_return("true"),
        ],
        hooks: Some(&ClassHooks {
            clone: Some(reject_clone),
            forbid_serialization: true,
            ..ClassHooks::EMPTY
        }),
        ..ClassEntry::class(CLASS)
    });
    for (entry, result) in [
        (
            F::asynchronous("finfo_open", &[DEFAULT_FLAGS, DATABASE], open),
            "finfo|false",
        ),
        (F::sync("finfo_close", &[FINFO], close), "true"),
        (
            F::sync("finfo_set_flags", &[FINFO, FLAGS], set_flags),
            "true",
        ),
        (
            F::sync(
                "finfo_buffer",
                &[FINFO, BUFFER, DEFAULT_FLAGS, CONTEXT],
                buffer,
            ),
            "string|false",
        ),
        (
            F::asynchronous(
                "finfo_file",
                &[FINFO, FILENAME, DEFAULT_FLAGS, CONTEXT],
                file,
            ),
            "string|false",
        ),
        (
            F::asynchronous("mime_content_type", &[P::new("filename", T::Untyped)], mime),
            "string|false",
        ),
    ] {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
}

fn object_error() -> NativeError {
    NativeError::error("Invalid finfo object")
}

fn reject_clone(object: &Rc<Object>, _: &Object) -> Result<(), NativeError> {
    Err(NativeError::error(format!(
        "Trying to clone an uncloneable object of class {}",
        object.class.name()
    )))
}

fn install(object: &Object, arguments: &[Value]) {
    *object.native.borrow_mut() = Some(Box::new(Cell::new(flags(arguments, 0))));
}

fn construct(
    context: &mut dyn Context,
    object: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = object.ok_or_else(object_error)?;
        *object.native.borrow_mut() = None;
        if let Some(database) = invalid_database(context, &arguments).await? {
            return Err(NativeError::throw(
                "Exception",
                format!("finfo::__construct{}", database.message()),
            ));
        }
        install(&object, &arguments);
        Ok(Value::Null)
    })
}

fn open(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        if let Some(database) = invalid_database(context, &arguments).await? {
            let message = database.message();
            context.warning(format!("finfo_open{message}"));
            if matches!(database, Database::Missing(_)) {
                context.warning(format!("finfo_open{message}"));
                context.warning(format!(
                    "finfo_open(): Failed to load magic database at \"{}\"",
                    database.path()
                ));
            }
            return Ok(Value::Bool(false));
        }
        let value = context.create_object(CLASS, PhpArray::new())?;
        if let Value::Object(object) = &value {
            install(object, &arguments);
        }
        Ok(value)
    })
}

enum Database {
    Missing(String),
    Unsupported(String),
}

impl Database {
    fn path(&self) -> &str {
        match self {
            Self::Missing(path) | Self::Unsupported(path) => path,
        }
    }

    fn message(&self) -> String {
        let path = self.path();
        match self {
            Self::Missing(_) => {
                format!("({path}): Failed to open stream: No such file or directory")
            }
            Self::Unsupported(_) => format!("({path}): Custom magic databases are unsupported"),
        }
    }
}

async fn invalid_database(
    context: &mut dyn Context,
    arguments: &[Value],
) -> Result<Option<Database>, NativeError> {
    use phera_ext_streams::file_io::{blocking, resolve};

    let path = args::bytes(arguments, 1);
    if path.is_empty() {
        return Ok(None);
    }
    let Ok(resolved) = resolve(context, path) else {
        return Ok(Some(Database::Missing(
            String::from_utf8_lossy(path).into(),
        )));
    };
    let display = resolved.display().to_string();
    let exists = blocking(move || resolved.exists()).await?;
    Ok(Some(if exists {
        Database::Unsupported(display)
    } else {
        Database::Missing(display)
    }))
}

fn close(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    context.deprecated(
        "Function finfo_close() is deprecated since 8.5, as finfo objects are freed automatically"
            .into(),
    );
    Ok(Value::Bool(true))
}

fn setting<T>(
    object: Option<&Rc<Object>>,
    f: impl FnOnce(&Cell<i32>) -> T,
) -> Result<T, NativeError> {
    let state = object.ok_or_else(object_error)?.native.borrow();
    state
        .as_ref()
        .and_then(|value| value.downcast_ref::<Cell<i32>>())
        .map(f)
        .ok_or_else(object_error)
}

fn set_flags(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    setting(args::object(arguments, 0), |cell| {
        cell.set(flags(arguments, 1))
    })?;
    Ok(Value::Bool(true))
}

fn method_set_flags(
    _: &mut dyn Context,
    object: Option<&Rc<Object>>,
    args: &[Value],
) -> NativeResult {
    setting(object, |cell| cell.set(flags(args, 0)))?;
    Ok(Value::Bool(true))
}

fn flags(arguments: &[Value], index: usize) -> i32 {
    args::int(arguments, index).unwrap_or_default() as i32
}

fn format(object: Option<&Rc<Object>>, arguments: &[Value]) -> Result<Format, NativeError> {
    let stored = setting(object, Cell::get)?;
    Ok(Format::new(match flags(arguments, 1) {
        0 => stored,
        value => value,
    }))
}

fn buffer(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    inspect_buffer(
        context,
        args::object(args, 0),
        &args[1..],
        "finfo_buffer",
        4,
    )
}

fn method_buffer(
    context: &mut dyn Context,
    object: Option<&Rc<Object>>,
    args: &[Value],
) -> NativeResult {
    inspect_buffer(context, object, args, "finfo::buffer", 3)
}

fn inspect_buffer(
    context: &mut dyn Context,
    object: Option<&Rc<Object>>,
    arguments: &[Value],
    name: &str,
    index: usize,
) -> NativeResult {
    if args::given(arguments, 2) {
        let value = args::value(arguments, 2);
        if !matches!(value, Value::Null | Value::Resource(_)) {
            return Err(NativeError::type_error(format!(
                "{name}(): Argument #{index} ($context) must be of type resource or null, {} given",
                given(value)
            )));
        }
        context.deprecated(format!(
            "{name}(): The $context parameter has no effect for finfo_buffer()"
        ));
    }
    Ok(Value::from(classifier::identify(
        args::bytes(arguments, 0),
        &[],
        format(object, arguments)?,
    )))
}

fn file(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        inspect_file(context, args::object(&args, 0), &args[1..], "finfo_file", 4).await
    })
}

fn method_file(
    context: &mut dyn Context,
    object: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { inspect_file(context, object.as_ref(), &args, "finfo::file", 3).await })
}

async fn inspect_file(
    context: &mut dyn Context,
    object: Option<&Rc<Object>>,
    arguments: &[Value],
    name: &str,
    index: usize,
) -> NativeResult {
    let format = format(object, arguments)?;
    let path = args::bytes(arguments, 0);
    if path.is_empty() {
        let parameter = if index == 3 { "flags" } else { "filename" };
        return Err(NativeError::value_error(format!(
            "{name}(): Argument #2 (${parameter}) must not be empty"
        )));
    }
    validate_stream_context(args::value(arguments, 2), name, index)?;
    let contents = inspect_path(context, name, path, LIMIT, 4).await?;
    Ok(identify_contents(contents, format))
}

fn mime(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        const NAME: &str = "mime_content_type";
        let contents = match args::value(&arguments, 0) {
            Value::String(path) => {
                if path.is_empty() {
                    return Err(NativeError::value_error(
                        "mime_content_type(): Argument #1 ($filename) must not be empty",
                    ));
                }
                if path.as_bytes().contains(&0) {
                    return Err(NativeError::type_error(
                        "mime_content_type(): Argument #1 ($filename) must not contain any null bytes",
                    ));
                }
                inspect_path(context, NAME, path.as_bytes(), LIMIT, 0).await?
            }
            Value::Resource(resource) if resource.kind == "stream" && !resource.is_closed() => {
                inspect_resource(resource, LIMIT).await?
            }
            Value::Resource(_) => {
                return Err(NativeError::type_error(
                    "mime_content_type(): supplied resource is not a valid stream resource",
                ));
            }
            other => {
                return Err(NativeError::type_error(format!(
                    "{NAME}(): Argument #1 ($filename) must be of type resource|string, {} given",
                    given(other)
                )));
            }
        };
        Ok(identify_contents(contents, Format::MimeType))
    })
}

fn given(value: &Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        other => other.type_name(),
    }
}

fn identify_contents(contents: Option<StreamContents>, format: Format) -> Value {
    match contents {
        Some(StreamContents::Directory) => Value::from("directory"),
        Some(StreamContents::Bytes { prefix, tail }) => {
            Value::from(classifier::identify(&prefix, &tail, format))
        }
        None => Value::Bool(false),
    }
}
