//! Open XMLReader inputs from PHP strings, files and streams.
//!
//! ```php
//! $reader = XMLReader::fromString('<root/>', encoding: 'UTF-8');
//! $reader->read();
//! ```

use std::path::Path;
use std::rc::Rc;

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, Value, WorkingDirectory, args,
};

use crate::state;

pub const CLASS: &str = "XMLReader";

pub fn xml(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(string(context, this, args, "XML", true))
}

pub fn from_string(
    context: &mut dyn Context,
    _: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(string(context, None, args, "fromString", false))
}

async fn string(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
    method: &str,
    legacy: bool,
) -> NativeResult {
    let source = nonempty(&args, method, "source")?.to_vec();
    let encoding = encoding(&args, method)?;
    let uri = default_uri(context);
    install(
        context,
        this,
        state::Input::Bytes(source),
        Initialization {
            uri,
            source: "",
            encoding,
            flags: flags(&args),
            legacy,
        },
    )
    .await
}

pub fn open(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(uri(context, this, args, "open", true))
}

pub fn from_uri(
    context: &mut dyn Context,
    _: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(uri(context, None, args, "fromUri", false))
}

async fn uri(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
    method: &str,
    legacy: bool,
) -> NativeResult {
    let source = nonempty(&args, method, "uri")?;
    let encoding = encoding(&args, method)?;
    let label = String::from_utf8_lossy(source);
    if source.contains(&0) {
        return Err(NativeError::value_error(format!(
            "XMLReader::{method}(): Argument #1 ($uri) must not contain any null bytes"
        )));
    }
    let path_bytes = source.strip_prefix(b"file://").unwrap_or(source);
    let path = phera_ext_streams::file_io::resolve(context, path_bytes);
    let data = match &path {
        Ok(path) => tokio::fs::read(path).await,
        Err(error) => Err(std::io::Error::from(error.kind())),
    };
    let input = match data {
        Ok(bytes) => state::Input::Bytes(bytes),
        Err(_) => {
            let function = format!("XMLReader::{method}");
            if legacy {
                context.warning(format!("{function}(): Unable to open source data"));
                return Ok(Value::Bool(false));
            }
            return Err(NativeError::error("Unable to open source data"));
        }
    };
    let uri = path
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    install(
        context,
        this,
        input,
        Initialization {
            uri,
            source: &label,
            encoding,
            flags: flags(&args),
            legacy,
        },
    )
    .await
}

pub fn from_stream(
    context: &mut dyn Context,
    _: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let stream = match args::value(&args, 0) {
            Value::Resource(resource) if phera_ext_streams::native::is_stream(resource) => {
                resource.clone()
            }
            Value::Resource(_) => {
                return Err(NativeError::type_error(
                    "XMLReader::fromStream(): supplied resource is not a valid stream resource",
                ));
            }
            other => {
                return Err(NativeError::type_error(format!(
                    "XMLReader::fromStream(): Argument #1 ($stream) must be of type resource, {} given",
                    other.type_name()
                )));
            }
        };
        let encoding = encoding(&args, "fromStream")?;
        let uri = args::string(&args, 3).map_or_else(
            || default_uri(context),
            phera_extension_api::PhpString::to_string_lossy,
        );
        install(
            context,
            None,
            state::Input::Stream(stream),
            Initialization {
                uri,
                source: "",
                encoding,
                flags: flags(&args),
                legacy: false,
            },
        )
        .await
    })
}

struct Initialization<'a> {
    uri: String,
    source: &'a str,
    encoding: Option<String>,
    flags: i64,
    legacy: bool,
}

async fn install(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    input: state::Input,
    configuration: Initialization<'_>,
) -> NativeResult {
    if let Some(object) = this {
        set_reader(&object, input, configuration);
        return Ok(Value::Bool(true));
    }
    let class = context.called_class().unwrap_or(CLASS).to_owned();
    let output = context.create_unserialized_object(&class)?;
    let Value::Object(object) = &output else {
        return Err(NativeError::error("Could not create XMLReader"));
    };
    if !configuration.legacy && context.has_method(object, "__construct") {
        context
            .call_method(object.clone(), "__construct", Vec::new())
            .await?;
    }
    set_reader(object, input, configuration);
    Ok(output)
}

fn set_reader(object: &Object, input: state::Input, config: Initialization<'_>) {
    state::install(
        object,
        state::State::new(
            input,
            config.uri,
            config.source.to_owned(),
            config.encoding,
            config.flags,
        ),
    );
}

fn nonempty<'a>(
    arguments: &'a [Value],
    method: &str,
    parameter: &str,
) -> Result<&'a [u8], NativeError> {
    let source = args::bytes(arguments, 0);
    if source.is_empty() {
        return Err(NativeError::value_error(format!(
            "XMLReader::{method}(): Argument #1 (${parameter}) must not be empty"
        )));
    }
    Ok(source)
}

fn encoding(args: &[Value], method: &str) -> Result<Option<String>, NativeError> {
    let name = args::string(args, 1).map(phera_extension_api::PhpString::to_string_lossy);
    if name.as_ref().is_some_and(|name| name.contains('\0')) {
        return Err(NativeError::value_error(format!(
            "XMLReader::{method}(): Argument #2 ($encoding) must not contain any null bytes"
        )));
    }
    if name
        .as_deref()
        .is_some_and(|name| !phera_ext_dom::xml::supported_encoding(name))
    {
        return Err(NativeError::value_error(format!(
            "XMLReader::{method}(): Argument #2 ($encoding) must be a valid character encoding"
        )));
    }
    Ok(name)
}

fn default_uri(context: &mut dyn Context) -> String {
    context
        .state()
        .get_or_default::<WorkingDirectory>()
        .path()
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .map(|directory| format!("{}/", directory.display().to_string().trim_end_matches('/')))
        .unwrap_or_default()
}

fn flags(args: &[Value]) -> i64 {
    phera_extension_api::args::int(args, 2).unwrap_or_default()
}
