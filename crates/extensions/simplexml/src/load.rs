//! Parse XML into DOM's arena or share a document that DOM already owns.
//!
//! ```php
//! $xml = simplexml_load_string('<root id="old"/>');
//! dom_import_simplexml($xml)->setAttribute('id', 'new');
//! echo $xml['id'];
//! ```

use std::path::PathBuf;
use std::rc::Rc;

use percent_encoding::percent_decode;
use phera_ext_dom::shared::{SharedNode, publish_xml_file_error, publish_xml_source_errors};
use phera_ext_dom::tree::{Arena, Kind};
use phera_ext_dom::xml::{self, Options};
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value, args};

use crate::selection::{CLASS, Filter, Selection, data, install, wrap};

pub fn construct(
    context: &mut dyn Context,
    object: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let Some(object) = object else {
            return Err(NativeError::error(
                "SimpleXMLElement is not properly initialized",
            ));
        };
        let function = "SimpleXMLElement::__construct";
        let source = args::bytes(&arguments, 0);
        let flags = args::int(&arguments, 1).unwrap_or_default();
        if i32::try_from(flags).is_err() {
            return Err(NativeError::throw(
                "Exception",
                "SimpleXMLElement::__construct(): Argument #2 ($options) is invalid",
            ));
        }
        let is_file = args::bool(&arguments, 2).unwrap_or(false);
        let Some(node) = load(context, function, source, flags, is_file).await? else {
            return Err(NativeError::throw(
                "Exception",
                "String could not be parsed as XML",
            ));
        };
        install(
            &object,
            Selection::node(node, selection_filter(&arguments, 3)),
        );
        Ok(Value::Null)
    })
}

pub fn load_string(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(load_function(context, arguments, false))
}

pub fn load_file(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(load_function(context, arguments, true))
}

async fn load_function(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    file: bool,
) -> NativeResult {
    let name = if file {
        "simplexml_load_file"
    } else {
        "simplexml_load_string"
    };
    let class = requested_class(context, &arguments, name).await?;
    let source = args::bytes(&arguments, 0);
    let flags = args::int(&arguments, 2).unwrap_or_default();
    if i32::try_from(flags).is_err() {
        return Err(NativeError::value_error(format!(
            "{name}(): Argument #3 ($options) is too large"
        )));
    }
    let selection = load(context, name, source, flags, file)
        .await?
        .map(|node| Selection::node(node, selection_filter(&arguments, 3)));
    match selection {
        Some(selection) => wrap(context, &class, selection),
        None => Ok(Value::Bool(false)),
    }
}

pub fn import_dom(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let class = requested_class(context, &arguments, "simplexml_import_dom").await?;
        let Some(node) =
            args::object(&arguments, 0).and_then(|object| SharedNode::from_dom(object))
        else {
            return Err(NativeError::type_error(
                "simplexml_import_dom(): Argument #1 ($node) must be a valid XML node",
            ));
        };
        if !node.owns_document() {
            context.warning(
                "simplexml_import_dom(): Imported Node must have associated Document".into(),
            );
            return Ok(Value::Null);
        }
        let selected = node.document(|doc| match doc.arena[node.id()].kind {
            Kind::Document => doc
                .arena
                .children(Arena::DOCUMENT)
                .find(|&id| doc.arena[id].kind == Kind::Element),
            Kind::Element | Kind::Attribute => Some(node.id()),
            _ => None,
        });
        let Some(selected) = selected.and_then(|id| node.select(id)) else {
            context.warning("simplexml_import_dom(): Invalid Nodetype to import".into());
            return Ok(Value::Null);
        };
        wrap(context, &class, Selection::node(selected, Filter::Default))
    })
}

pub fn import_simplexml(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let node = args::object(arguments, 0)
        .and_then(|object| data(object).ok())
        .and_then(|state| state.selection.first());
    let Some(node) = node else {
        return Err(NativeError::type_error(
            "dom_import_simplexml(): Argument #1 ($node) is not a valid node type",
        ));
    };
    node.export_dom(context)
}

async fn requested_class(
    context: &mut dyn Context,
    arguments: &[Value],
    name: &str,
) -> Result<String, NativeError> {
    let Some(class) = args::string(arguments, 1) else {
        return Ok(CLASS.to_owned());
    };
    let class = String::from_utf8_lossy(class.as_bytes());
    if let Some(linked) = context.lookup_class(&class).await?
        && linked.is_instance_of(CLASS)
    {
        return Ok(linked.name().to_owned());
    }
    Err(NativeError::type_error(format!(
        "{name}(): Argument #2 ($class_name) must be a class name derived from SimpleXMLElement or null, {class} given"
    )))
}

async fn load(
    context: &mut dyn Context,
    function: &str,
    input: &[u8],
    flags: i64,
    file: bool,
) -> Result<Option<SharedNode>, NativeError> {
    let display = String::from_utf8_lossy(input);
    let file_bytes;
    let (input, source) = if file {
        let parameter = if function == "SimpleXMLElement::__construct" {
            "data"
        } else {
            "filename"
        };
        let path = resolve_file(context, input, function, parameter)?;
        file_bytes = match path {
            Some(path) => tokio::fs::read(path).await.ok(),
            None => None,
        };
        let Some(bytes) = file_bytes.as_deref() else {
            publish_xml_file_error(context, function, &display);
            return Ok(None);
        };
        (bytes, display.as_ref())
    } else {
        (input, "")
    };
    let parsed = xml::parse(
        input,
        Options {
            substitute_entities: flags & 2 != 0,
            strip_blanks: flags & 256 != 0,
            merge_cdata: flags & 16384 != 0,
            default_attributes: flags & 8 != 0,
            recover: flags & 1 != 0,
            huge: flags & 524288 != 0,
        },
    );
    publish_xml_source_errors(context, function, source, input, parsed.diagnostics, flags);
    Ok(parsed.document.and_then(SharedNode::root))
}

pub fn resolve_file(
    context: &mut dyn Context,
    name: &[u8],
    function: &str,
    parameter: &str,
) -> Result<Option<PathBuf>, NativeError> {
    if name.contains(&0) {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 (${parameter}) must not contain any null bytes"
        )));
    }
    let decoded = percent_decode(name).collect::<Vec<_>>();
    if decoded.contains(&0) {
        context.warning(format!(
            "{function}(): URI must not contain percent-encoded NUL bytes"
        ));
        return Ok(None);
    }
    let path = match decoded.strip_prefix(b"file://") {
        Some(rest) if rest.starts_with(b"/") => rest,
        Some(rest) if rest.starts_with(b"localhost/") => &rest[b"localhost".len()..],
        Some(_) => return Ok(None),
        None => &decoded,
    };
    Ok(phera_ext_streams::file_io::resolve(context, path).ok())
}

pub fn selection_filter(arguments: &[Value], start: usize) -> Filter {
    let label = args::string(arguments, start).map(|name| String::from_utf8_lossy(name.as_bytes()));
    Filter::new(
        label.as_deref(),
        args::bool(arguments, start + 1).unwrap_or(false),
    )
}
