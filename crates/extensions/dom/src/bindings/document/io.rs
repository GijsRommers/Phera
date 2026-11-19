//! Parse strings synchronously and use Tokio for explicit file operations.
//!
//! ```php
//! $document->load('input.xml', LIBXML_NONET);
//! $document->save('output.xml');
//! ```

use std::path::{Path, PathBuf};
use std::rc::Rc;

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, Value, WorkingDirectory, args,
};

use crate::document::{Document, Settings};
use crate::html;
use crate::serialize::{self, Options};
use crate::tree::Arena;
use crate::xml;

use super::super::{Location, This, dom_error, libxml, location, receiver, this_node};

#[derive(Clone, Copy)]
enum Format {
    Xml,
    Html,
}

impl Format {
    fn html(self) -> bool {
        matches!(self, Self::Html)
    }
}

pub(super) fn load_xml(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    load_string(context, this, args, Format::Xml, "DOMDocument::loadXML")
}

pub(super) fn load_html(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    load_string(context, this, args, Format::Html, "DOMDocument::loadHTML")
}

fn load_string(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
    format: Format,
    function: &str,
) -> NativeResult {
    let bytes = args::bytes(arguments, 0);
    if bytes.is_empty() {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($source) must not be empty"
        )));
    }
    let base = context
        .state()
        .get_or_default::<WorkingDirectory>()
        .path()
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .map(|path| format!("{}/", path.display().to_string().trim_end_matches('/')));
    install(
        context,
        receiver(this)?,
        bytes,
        ParseInput {
            flags: args::int(arguments, 1).unwrap_or_default(),
            format,
            function,
            uri: base,
            file: "",
        },
    )
}

struct ParseInput<'a> {
    flags: i64,
    format: Format,
    function: &'a str,
    uri: Option<String>,
    file: &'a str,
}

fn install(
    context: &mut dyn Context,
    object: &Rc<Object>,
    bytes: &[u8],
    input: ParseInput<'_>,
) -> NativeResult {
    let original = location(object)?;
    let previous = original.shared.borrow().document.settings.clone();
    let document = parse(context, bytes, &input, &previous);
    let Some(mut document) = document else {
        return Ok(Value::Bool(false));
    };
    document.settings = Settings {
        uri: input.uri,
        format: previous.format,
        strict: previous.strict,
        preserve_whitespace: previous.preserve_whitespace,
        recover: previous.recover,
        resolve_externals: previous.resolve_externals,
        substitute_entities: previous.substitute_entities,
        validate_on_parse: previous.validate_on_parse,
        ..document.settings
    };
    let location = Location::new(document, true, Arena::DOCUMENT);
    {
        let mut state = original.shared.borrow_mut();
        state.wrappers.remove(&original.id);
        location
            .shared
            .borrow_mut()
            .node_classes
            .clone_from(&state.node_classes);
    }
    location.bind(object);
    Ok(Value::Bool(true))
}

fn parse(
    context: &mut dyn Context,
    bytes: &[u8],
    input: &ParseInput<'_>,
    previous: &Settings,
) -> Option<Document> {
    let flags = input.flags;
    let strip_blanks = !previous.preserve_whitespace || (flags & 256) != 0;
    match input.format {
        Format::Xml => {
            let parsed = xml::parse(
                bytes,
                xml::Options {
                    substitute_entities: previous.substitute_entities || (flags & 2) != 0,
                    strip_blanks,
                    merge_cdata: (flags & 16384) != 0,
                    default_attributes: (flags & 8) != 0,
                    recover: previous.recover || (flags & 1) != 0,
                    huge: (flags & 524288) != 0,
                },
            );
            libxml::publish(
                context,
                input.function,
                input.file,
                parsed.diagnostics,
                flags,
            );
            parsed.document
        }
        Format::Html => Some(html::parse(
            bytes,
            html::Options {
                omit_implied: (flags & 8192) != 0,
                omit_default_doctype: (flags & 4) != 0,
                strip_blanks,
            },
        )),
    }
}

pub(super) fn load_file<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    load_from_file(context, this, args, Format::Xml, "DOMDocument::load")
}

pub(super) fn load_html_file<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    load_from_file(
        context,
        this,
        args,
        Format::Html,
        "DOMDocument::loadHTMLFile",
    )
}

fn load_from_file<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    format: Format,
    function: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let (name, path) = filename(context, &arguments, function)?;
        let limit = if cfg!(target_os = "linux") {
            4096
        } else {
            1024
        };
        if name.len() >= limit || path.as_os_str().len() >= limit {
            return Ok(Value::Bool(false));
        }
        let flags = args::int(&arguments, 1).unwrap_or_default();
        match tokio::fs::read(&path).await {
            Ok(bytes) => {
                let path = path.to_string_lossy();
                install(
                    context,
                    receiver(this.as_ref())?,
                    &bytes,
                    ParseInput {
                        flags,
                        format,
                        function,
                        uri: Some(path.to_string()),
                        file: &path,
                    },
                )
            }
            Err(_) => {
                let display = if format.html() {
                    name
                } else {
                    path.to_string_lossy().into_owned()
                };
                libxml::file_failure(context, function, &display);
                Ok(Value::Bool(false))
            }
        }
    })
}

pub(in crate::bindings) fn save_xml(
    _: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    Ok(Value::from(serialized(
        this,
        args::object(arguments, 0),
        args::int(arguments, 1).unwrap_or_default(),
        Format::Xml,
    )?))
}

pub(in crate::bindings) fn save_html(
    _: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    Ok(Value::from(serialized(
        this,
        args::object(arguments, 0),
        0,
        Format::Html,
    )?))
}

fn serialized(
    this: This<'_>,
    object: Option<&Rc<Object>>,
    flags: i64,
    format: Format,
) -> Result<Vec<u8>, NativeError> {
    let original = this_node(this)?;
    let target = object.map(|object| location(object)).transpose()?;
    if let Some(target) = &target
        && !target.same_document(&original)
    {
        return Err(dom_error(4, "Wrong Document Error"));
    }
    let document = original.shared.borrow();
    Ok(serialize::save(
        &document.document,
        target.map_or(Arena::DOCUMENT, |target| target.id),
        Options {
            html: format.html(),
            omit_declaration: (flags & 2) != 0,
            expand_empty: (flags & 4) != 0,
        },
    ))
}

pub(in crate::bindings) fn save_file<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    save_to_file(context, this, args, Format::Xml, "DOMDocument::save")
}

pub(in crate::bindings) fn save_html_file<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    save_to_file(
        context,
        this,
        args,
        Format::Html,
        "DOMDocument::saveHTMLFile",
    )
}

fn save_to_file<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    format: Format,
    function: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let (name, path) = filename(context, &arguments, function)?;
        if format.html() {
            let target = this_node(this.as_ref())?;
            let mut state = target.shared.borrow_mut();
            if !state.document.settings.modern {
                super::html_file::ensure_content_type(&mut state.document.arena);
            }
        }
        let bytes = serialized(
            this.as_ref(),
            None,
            args::int(&arguments, 1).unwrap_or_default(),
            format,
        )?;
        match tokio::fs::write(path, &bytes).await {
            Ok(()) => Ok(Value::Int(bytes.len() as i64)),
            Err(error) => {
                let message = error.to_string();
                let message = message.split(" (os error ").next().unwrap_or(&message);
                context.warning(format!(
                    "{function}({name}): Failed to open stream: {message}"
                ));
                Ok(if format.html() {
                    Value::Int(0)
                } else {
                    Value::Bool(false)
                })
            }
        }
    })
}

pub(in crate::bindings) fn filename(
    context: &mut dyn Context,
    args: &[Value],
    function: &str,
) -> Result<(String, PathBuf), NativeError> {
    let bytes = phera_extension_api::args::bytes(args, 0);
    let reason = if bytes.is_empty() {
        Some("must not be empty")
    } else if bytes.contains(&0) {
        Some("must not contain any null bytes")
    } else {
        None
    };
    if let Some(reason) = reason {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($filename) {reason}"
        )));
    }
    let display = String::from_utf8_lossy(bytes).into_owned();
    let path = context
        .state()
        .get_or_default::<WorkingDirectory>()
        .resolve(Path::new(&display))
        .map_err(|error| NativeError::error(error.to_string()))?;
    Ok((display, path))
}
