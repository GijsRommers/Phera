//! Initialize writers with memory, file, and PHP stream destinations.
//!
//! ```php
//! $writer = XMLWriter::toUri('output.xml');
//! $writer->writeElement('root');
//! $writer->endDocument();
//! ```

use std::ffi::OsStr;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::rc::Rc;

use percent_encoding::percent_decode;
use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args,
};

use crate::output::{self, Target};

pub const CLASS: &str = "XMLWriter";

pub fn open_memory(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(open(context, this, Target::Memory))
}

pub fn procedural_open_memory(
    context: &mut dyn Context,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(open(context, None, Target::Memory))
}

pub fn to_memory(
    context: &mut dyn Context,
    _: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(create(context, Target::Memory))
}

pub fn open_uri(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(legacy_uri(context, this, arguments, "XMLWriter::openUri"))
}

pub fn procedural_open_uri(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(legacy_uri(context, None, arguments, "xmlwriter_open_uri"))
}

pub fn to_uri(
    context: &mut dyn Context,
    _: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "XMLWriter::toUri";
        let target = uri(context, function, args::bytes(&arguments, 0)).await?;
        match target {
            Some(target) => create(context, target).await,
            None => Err(NativeError::value_error(
                "XMLWriter::toUri(): Argument #1 ($uri) must resolve to a valid file path",
            )),
        }
    })
}

pub fn to_stream(
    context: &mut dyn Context,
    _: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let stream = match args::value(&arguments, 0) {
            Value::Resource(resource) if phera_ext_streams::native::is_stream(resource) => {
                resource.clone()
            }
            Value::Resource(_) => {
                return Err(NativeError::type_error(
                    "XMLWriter::toStream(): supplied resource is not a valid stream resource",
                ));
            }
            value => {
                return Err(NativeError::type_error(format!(
                    "XMLWriter::toStream(): Argument #1 ($stream) must be of type resource, {} given",
                    value.type_name()
                )));
            }
        };
        create(context, Target::Stream(stream)).await
    })
}

async fn open(context: &mut dyn Context, this: Option<Rc<Object>>, target: Target) -> NativeResult {
    if let Some(object) = this {
        if let Ok(previous) = output::state(&object) {
            output::publish(context, &previous, "XMLWriter").await;
        }
        output::install(context, &object, target);
        Ok(Value::Bool(true))
    } else {
        let value = context.create_object(CLASS, PhpArray::new())?;
        let Some(object) = args::object(std::slice::from_ref(&value), 0) else {
            return Err(output::uninitialized());
        };
        output::install(context, object, target);
        Ok(value)
    }
}

async fn create(context: &mut dyn Context, target: Target) -> NativeResult {
    let class = context.called_class().unwrap_or(CLASS).to_owned();
    let value = context.create_unserialized_object(&class)?;
    let Value::Object(ref object) = value else {
        return Err(output::uninitialized());
    };
    if context.has_method(object, "__construct") {
        context
            .call_method(object.clone(), "__construct", Vec::new())
            .await?;
    }
    output::install(context, object, target);
    Ok(value)
}

async fn legacy_uri(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    function: &str,
) -> NativeResult {
    match uri(context, function, args::bytes(&arguments, 0)).await? {
        Some(target) => open(context, this, target).await,
        None => {
            context.warning(format!("{function}(): Unable to resolve file path"));
            Ok(Value::Bool(false))
        }
    }
}

async fn uri(
    context: &mut dyn Context,
    function: &str,
    name: &[u8],
) -> Result<Option<Target>, NativeError> {
    if name.is_empty() || name.contains(&0) {
        let requirement = if name.is_empty() {
            "be empty"
        } else {
            "contain any null bytes"
        };
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($uri) must not {requirement}"
        )));
    }
    if name.starts_with(b"php://") {
        let stream = context
            .call(
                &Value::from("fopen"),
                vec![Value::from(name.to_vec()), Value::from("wb")],
            )
            .await?;
        return Ok(match stream {
            Value::Resource(resource) => Some(Target::Stream(resource)),
            _ => None,
        });
    }
    let name = match name.strip_prefix(b"file://") {
        Some(rest) if rest.starts_with(b"/") => rest,
        Some(rest) if rest.starts_with(b"localhost/") => &rest[b"localhost".len()..],
        Some(_) => return Ok(None),
        None => name,
    };
    let name = percent_decode(name).collect::<Vec<_>>();
    if name.contains(&0) || name.ends_with(b"/") {
        return Ok(None);
    }
    let Ok(path) = phera_ext_streams::file_io::resolve(context, &name) else {
        return Ok(None);
    };
    if Path::new(OsStr::from_bytes(&name)).file_name().is_none() {
        return Ok(None);
    }
    match tokio::fs::File::create(path).await {
        Ok(file) => Ok(Some(Target::Uri(Rc::new(tokio::sync::Mutex::new(file))))),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::NotFound
                    | ErrorKind::NotADirectory
                    | ErrorKind::IsADirectory
                    | ErrorKind::InvalidInput
                    | ErrorKind::InvalidFilename
            ) =>
        {
            Ok(None)
        }
        Err(_) => {
            context.warning(format!(
                "{function}(): xmlNewTextWriterFilename : cannot open uri"
            ));
            if function == "XMLWriter::toUri" {
                Err(NativeError::error("Could not construct libxml writer"))
            } else {
                Ok(None)
            }
        }
    }
}
