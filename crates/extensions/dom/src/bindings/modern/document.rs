//! Initialize and save modern DOM documents through PHP's static constructors.
//!
//! ```php
//! $document = Dom\XMLDocument::createFromString('<root/>');
//! echo $document->saveXml();
//! ```

mod registration;

pub(super) use registration::register;

use std::path::Path;
use std::rc::Rc;

use encoding_rs::Encoding;
use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, Value, WorkingDirectory, args,
};

use crate::document::{Document, Settings};
use crate::tree::{Arena, Kind, Name, Node};
use crate::{html, xml};

use super::super::{Location, This, dom_error, factory, libxml, text, this_node};

#[derive(Clone, Copy)]
enum Format {
    Html,
    Xml,
}

impl Format {
    fn class(self) -> &'static str {
        match self {
            Self::Html => "Dom\\HTMLDocument",
            Self::Xml => "Dom\\XMLDocument",
        }
    }
}

fn create_empty(context: &mut dyn Context, arguments: &[Value], format: Format) -> NativeResult {
    let html = matches!(format, Format::Html);
    let encoding = factory::argument(arguments, usize::from(!html)).unwrap_or("UTF-8".into());
    if Encoding::for_label(encoding.as_bytes()).is_none() && !xml::supported_encoding(&encoding) {
        return Err(NativeError::value_error(format!(
            "{}::createEmpty(): Argument #{} ($encoding) must be a valid document encoding",
            format.class(),
            if html { 1 } else { 2 },
        )));
    }
    let version = if html {
        "1.0".into()
    } else {
        factory::argument(arguments, 0).unwrap_or("1.0".into())
    };
    let document = Document {
        settings: Settings {
            modern: true,
            html,
            standalone: html.then_some(true),
            version: Some(version),
            encoding: Some(encoding),
            uri: Some("about:blank".into()),
            ..Settings::default()
        },
        ..Document::default()
    };
    Location::new(document, true, Arena::DOCUMENT).wrap(context)
}

fn from_string(context: &mut dyn Context, arguments: &[Value], format: Format) -> NativeResult {
    let bytes = args::bytes(arguments, 0);
    let function = format!("{}::createFromString", format.class());
    if bytes.is_empty() {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($source) must not be empty",
        )));
    }
    let base = if matches!(format, Format::Xml) {
        context
            .state()
            .get_or_default::<WorkingDirectory>()
            .path()
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .map(|path| format!("{}/", path.to_string_lossy().trim_end_matches('/')))
    } else {
        None
    };
    parse(context, bytes, arguments, format, &function, "", base)
}

fn from_file(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    format: Format,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = format!("{}::createFromFile", format.class());
        let file = args::bytes(&arguments, 0);
        if file.is_empty() || file.contains(&0) {
            let reason = if file.is_empty() {
                "must not be empty"
            } else {
                "must not contain any null bytes"
            };
            return Err(NativeError::value_error(format!(
                "{function}(): Argument #1 ($path) {reason}"
            )));
        }
        let file = String::from_utf8_lossy(file);
        let path = context
            .state()
            .get_or_default::<WorkingDirectory>()
            .resolve(Path::new(&*file))
            .map_err(|error| NativeError::error(error.to_string()))?;
        let path_display = path.to_string_lossy();
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(_) => {
                libxml::file_failure(context, &function, &path_display);
                return Err(dom_error(12, "Cannot open file"));
            }
        };
        parse(
            context,
            &bytes,
            &arguments,
            format,
            &function,
            &path_display,
            Some(path_display.to_string()),
        )
    })
}

fn parse(
    context: &mut dyn Context,
    bytes: &[u8],
    arguments: &[Value],
    format: Format,
    function: &str,
    file: &str,
    uri: Option<String>,
) -> NativeResult {
    let flags = args::int(arguments, 1).unwrap_or_default();
    let override_encoding = factory::argument(arguments, 2);
    let mut document = if matches!(format, Format::Html) {
        let valid = 32 | 65536 | 8192 | (1 << 31);
        if flags & !valid != 0 {
            return Err(NativeError::value_error(format!(
                "{function}(): Argument #2 ($options) contains invalid flags \
                (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, \
                Dom\\HTML_NO_DEFAULT_NS)",
            )));
        }
        let encoding = override_encoding
            .as_ref()
            .and_then(|name| Encoding::for_label(name.as_bytes()));
        if encoding.is_none() && override_encoding.is_some() {
            return Err(NativeError::value_error(format!(
                "{function}(): Argument #3 ($overrideEncoding) must be a valid document encoding",
            )));
        }
        html::parse_modern(
            bytes,
            encoding,
            (flags & 8192) != 0,
            (flags & (1 << 31)) != 0,
        )
    } else {
        let parsed = xml::parse_with_encoding(
            bytes,
            xml::Options {
                substitute_entities: flags & 2 != 0,
                strip_blanks: flags & 256 != 0,
                merge_cdata: flags & 16384 != 0,
                default_attributes: flags & 8 != 0,
                recover: flags & 1 != 0,
                huge: flags & 524288 != 0,
            },
            override_encoding.as_deref(),
        );
        libxml::publish(context, function, file, parsed.diagnostics, flags);
        let mut document = parsed
            .document
            .ok_or_else(|| dom_error(12, "XML fragment is not well-formed"))?;
        if document.settings.encoding.is_none() {
            document.settings.encoding = Some(override_encoding.unwrap_or("UTF-8".into()));
        }
        document
    };
    document.settings.modern = true;
    if uri.is_some() {
        document.settings.uri = uri;
    }
    Location::new(document, true, Arena::DOCUMENT).wrap(context)
}

pub(super) fn property(context: &mut dyn Context, node: Location, name: &str) -> NativeResult {
    let state = node.shared.borrow();
    let document = &state.document;
    match name {
        "URL" | "documentURI" => {
            return Ok(Value::from(
                document.settings.uri.as_deref().unwrap_or("about:blank"),
            ));
        }
        "characterSet" | "charset" | "inputEncoding" => {
            return Ok(Value::from(
                document.settings.encoding.as_deref().unwrap_or("UTF-8"),
            ));
        }
        _ => {}
    }
    let arena = &document.arena;
    let part = if name == "title" { "head" } else { name };
    let selected = arena.content_descendants(Arena::DOCUMENT).find(|&id| {
        let element = &arena[id];
        element.kind == Kind::Element
            && element.name.namespace.as_deref() == Some(html::XHTML)
            && (element.name.local == part || (part == "body" && element.name.local == "frameset"))
    });
    if name == "title" {
        let title = selected.and_then(|id| {
            arena
                .children(id)
                .find(|&candidate| arena[candidate].name.local == "title")
        });
        return Ok(Value::from(
            title
                .map(|id| text(arena, id))
                .unwrap_or_default()
                .split_ascii_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }
    drop(state);
    node.wrap_optional(context, selected)
}

fn create_node(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
    kind: Kind,
    qualified: bool,
) -> NativeResult {
    let document = this_node(this)?;
    let id = {
        let mut state = document.shared.borrow_mut();
        let mut name = factory::checked_name(
            factory::argument(arguments, usize::from(qualified)).unwrap_or_default(),
            qualified
                .then(|| factory::namespace(arguments, 0))
                .flatten(),
            qualified,
        )?;
        if !qualified && state.document.settings.html {
            name.local.make_ascii_lowercase();
            if kind == Kind::Element {
                name = Name {
                    namespace: Some(html::XHTML.into()),
                    ..name
                };
            }
        }
        if state.document.settings.html {
            state.document.arena.allocate(Node::new(kind, name, ""))
        } else {
            factory::insert_new(&mut state.document.arena, Node::new(kind, name, ""))
        }
    };
    document.at(id).wrap(context)
}

fn new_html_file(
    context: &mut dyn Context,
    _: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    from_file(context, args, Format::Html)
}

fn new_xml_file(
    context: &mut dyn Context,
    _: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    from_file(context, args, Format::Xml)
}
