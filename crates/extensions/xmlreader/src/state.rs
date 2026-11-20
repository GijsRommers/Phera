//! Defer XMLReader parsing until read() selects the first event.
//!
//! ```php
//! $reader = XMLReader::fromString('<r/>');
//! $reader->setParserProperty(XMLReader::SUBST_ENTITIES, true);
//! $reader->read();
//! ```

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use phera_ext_dom::shared::{SharedNode, publish_xml_source_errors};
use phera_ext_dom::xml::{self, Options};
use phera_extension_api::{
    Context, NativeError, Object, Value,
    value::{Fork, Resource},
};

use crate::cursor::Position;

pub type Reader = Rc<RefCell<State>>;

#[derive(Clone)]
pub enum Input {
    Bytes(Vec<u8>),
    Stream(Rc<Resource>),
}

impl Input {
    pub async fn read(self, context: &mut dyn Context) -> Result<Vec<u8>, NativeError> {
        match self {
            Self::Bytes(bytes) => Ok(bytes),
            Self::Stream(stream) if !stream.is_closed() => {
                match context
                    .call(
                        &Value::from("stream_get_contents"),
                        vec![Value::Resource(stream)],
                    )
                    .await?
                {
                    Value::String(content) => Ok(content.as_bytes().to_vec()),
                    _ => Ok(Vec::new()),
                }
            }
            Self::Stream(_) => Ok(Vec::new()),
        }
    }
}

#[derive(Clone)]
pub struct State {
    source: Option<Input>,
    source_name: String,
    encoding: Option<String>,
    pub uri: String,
    flags: i64,
    stream: Option<Rc<Resource>>,
    pub tree: Option<SharedNode>,
    pub position: Position,
    pub load_dtd: bool,
    pub default_attributes: bool,
    pub validate: bool,
    pub substitute: bool,
    pub incomplete_stream: bool,
}

impl State {
    pub fn new(
        source: Input,
        uri: String,
        source_name: String,
        encoding: Option<String>,
        flags: i64,
    ) -> Self {
        let stream = match &source {
            Input::Stream(stream) => Some(stream.clone()),
            Input::Bytes(_) => None,
        };
        Self {
            source: Some(source),
            source_name,
            encoding,
            uri,
            flags,
            stream,
            tree: None,
            position: Position::Before,
            load_dtd: flags & 4 != 0,
            default_attributes: flags & 8 != 0,
            validate: flags & 16 != 0,
            substitute: flags & 2 != 0,
            incomplete_stream: false,
        }
    }

    pub fn take_source(&mut self) -> Option<Input> {
        self.source.take()
    }

    pub fn parse(&mut self, context: &mut dyn Context, method: &str, source: &[u8]) {
        if source.is_empty() {
            self.position = Position::After;
            return;
        }
        let mut parsed = xml::parse_with_encoding(
            source,
            Options {
                substitute_entities: self.substitute,
                strip_blanks: self.flags & 256 != 0,
                merge_cdata: self.flags & 16384 != 0,
                default_attributes: self.default_attributes,
                recover: self.flags & 1 != 0 || self.stream.is_some(),
                huge: self.flags & 524288 != 0,
            },
            self.encoding.as_deref(),
        );
        self.incomplete_stream = self.stream.is_some()
            && parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == 77);
        let function = format!("XMLReader::{method}");
        publish_xml_source_errors(
            context,
            &function,
            &self.source_name,
            source,
            parsed.diagnostics,
            self.flags,
        );
        if let Some(document) = &mut parsed.document {
            document.settings.uri = Some(self.uri.clone());
        }
        self.tree = parsed.document.and_then(SharedNode::root);
        if self.tree.is_none() {
            self.position = Position::After;
        }
    }

    pub fn started(&self) -> bool {
        self.source.is_none()
    }
}

pub fn data(object: &Object) -> Option<Reader> {
    object
        .native
        .borrow()
        .as_ref()?
        .downcast_ref::<Reader>()
        .cloned()
}

pub fn fork(_: &Object, native: &dyn Any, fork: &mut Fork<'_>) -> Result<Box<dyn Any>, String> {
    let reader = native
        .downcast_ref::<Reader>()
        .ok_or("its XML reader is not initialized")?;
    let source = reader.borrow();
    if source
        .stream
        .as_ref()
        .is_some_and(|stream| !stream.is_closed())
    {
        return Err(
            "its input stream belongs to one request; open it after Phera\\snapshot()".into(),
        );
    }
    let mut copy = source.clone();
    copy.tree = source.tree.as_ref().map(|node| node.fork(fork));
    Ok(Box::new(Rc::new(RefCell::new(copy))))
}

pub fn install(object: &Object, reader: State) {
    *object.native.borrow_mut() = Some(Box::new(Rc::new(RefCell::new(reader))));
}
