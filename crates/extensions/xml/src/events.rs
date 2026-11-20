//! Turn complete XML tokens into PHP SAX events.
//!
//! ```php
//! xml_set_element_handler($parser, $start, $end);
//! xml_parse($parser, '<root>');
//! xml_parse($parser, '<item/></root>', true);
//! ```

use std::collections::HashMap;

use phera_ext_dom::tree::DtdEntry;
use phera_ext_dom::xml::EntityResolver;
use phera_extension_api::Value;
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::options::{Codec, Options};

mod doctype;
mod elements;
mod fallback;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Handler {
    Start,
    End,
    Text,
    Pi,
    Default,
    Namespace,
    NamespaceEnd,
    Notation,
    Unparsed,
    External,
}

pub struct Emission {
    pub handler: Handler,
    pub data: Vec<Value>,
    pub position: Position,
    pub fallback: Option<Value>,
}

#[derive(Clone, Copy)]
pub struct Position {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

impl Default for Position {
    fn default() -> Self {
        Self {
            offset: 0,
            line: 1,
            column: 1,
        }
    }
}

impl Position {
    fn walked(mut self, bytes: &[u8]) -> Self {
        self.offset += bytes.len();
        for &byte in bytes {
            if byte == b'\n' {
                self.line += 1;
                self.column = 1;
            } else if byte & 0xc0 != 0x80 {
                self.column += 1;
            }
        }
        self
    }
}

struct Open {
    qualified: String,
    scoped: String,
    namespaces: Vec<(String, Option<String>)>,
}

pub struct Parser {
    pub options: Options,
    pub error: i64,
    pub position: Position,
    cursor: Position,
    consumed: usize,
    input: Vec<u8>,
    elements: Vec<Open>,
    namespaces: HashMap<String, String>,
    namespace_separator: Option<String>,
    entities: EntityResolver,
    external: HashMap<String, DtdEntry>,
    attribute_defaults: HashMap<String, Vec<(String, String)>>,
    bom: bool,
    root: bool,
    finished: bool,
}

impl Parser {
    pub fn new(source: Option<Codec>, separator: Option<String>) -> Self {
        Self {
            options: Options::new(source),
            error: 0,
            position: Position::default(),
            cursor: Position::default(),
            consumed: 0,
            input: Vec::new(),
            elements: Vec::new(),
            namespaces: HashMap::from([(
                "xml".to_owned(),
                "http://www.w3.org/XML/1998/namespace".to_owned(),
            )]),
            namespace_separator: separator,
            entities: EntityResolver::default(),
            external: HashMap::new(),
            attribute_defaults: HashMap::new(),
            bom: false,
            root: false,
            finished: false,
        }
    }

    pub fn append(&mut self, bytes: &[u8]) -> bool {
        if self.finished {
            if self.error == 0 {
                self.error = 5;
            }
            return false;
        }
        if matches!(self.options.source, Some(Codec::Latin1)) {
            self.input.extend(latin1(bytes));
        } else {
            self.input.extend_from_slice(bytes);
        }
        true
    }

    pub fn next(
        &mut self,
        final_chunk: bool,
        has_default: bool,
    ) -> Option<Result<Vec<Emission>, i64>> {
        if self.finished {
            return None;
        }
        const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";
        if self.cursor.offset == 0 {
            if self.input.starts_with(UTF8_BOM) {
                self.consumed += UTF8_BOM.len();
                self.cursor.offset += UTF8_BOM.len();
                self.bom = true;
            } else if !final_chunk && UTF8_BOM.starts_with(&self.input) {
                return None;
            }
        }
        let remaining = &self.input[self.consumed..];
        if remaining.is_empty() {
            if !final_chunk {
                return None;
            }
            self.finished = true;
            if !self.elements.is_empty() || !self.root {
                self.error = if self.root { 77 } else { 4 };
                return Some(Err(self.error));
            }
            return None;
        }
        let mut reader = Reader::from_reader(remaining);
        let config = reader.config_mut();
        config.check_end_names = false;
        config.allow_unmatched_ends = true;
        config.check_comments = true;
        let event = reader.read_event();
        let length = reader.buffer_position() as usize;
        if !final_chunk
            && (event.is_err() || matches!(&event, Ok(Event::Text(_)) if length == remaining.len()))
        {
            return None;
        }
        if length == 0 {
            if !final_chunk {
                return None;
            }
            self.finished = true;
            self.error = 9;
            return Some(Err(self.error));
        }
        let raw = remaining.get(..length).unwrap_or_default().to_vec();
        let location = self.cursor;
        self.consumed += length;
        self.cursor = self.cursor.walked(&raw);
        self.position = self.cursor;
        let result = match event {
            _ if matches!(self.options.source, Some(Codec::Ascii)) && !raw.is_ascii() => Err(9),
            Ok(event) => self.handle(event.into_owned(), &raw, location, has_default),
            Err(error) => Err(match error {
                quick_xml::Error::IllFormed(
                    quick_xml::errors::IllFormedError::UnclosedReference,
                ) => 23,
                quick_xml::Error::Encoding(_) => 9,
                _ => 73,
            }),
        };
        if let Err(error) = result {
            self.error = error;
            self.finished = true;
        }
        Some(result)
    }

    pub fn compact(&mut self) {
        self.input.drain(..self.consumed);
        self.consumed = 0;
        self.position = self.cursor;
    }

    fn handle(
        &mut self,
        event: Event<'_>,
        raw: &[u8],
        start: Position,
        has_default: bool,
    ) -> Result<Vec<Emission>, i64> {
        let mut events = Vec::new();
        match event {
            Event::Start(tag) => self.open(
                &tag,
                start.walked(&raw[..raw.len().saturating_sub(1)]),
                &mut events,
            )?,
            Event::Empty(tag) => {
                self.open(
                    &tag,
                    start.walked(&raw[..raw.len().saturating_sub(2)]),
                    &mut events,
                )?;
                self.close(tag.name().as_ref(), &mut events)?;
            }
            Event::End(tag) => self.close(tag.name().as_ref(), &mut events)?,
            Event::Text(text) => {
                let text = text.into_inner();
                if self.elements.is_empty() {
                    if !text.trim().is_empty() {
                        return Err(if self.root { 5 } else { 4 });
                    }
                } else if !text.is_empty() {
                    let position = Position {
                        offset: start.offset,
                        ..self.cursor
                    };
                    self.characters(&text, position, &mut events);
                }
            }
            Event::CData(text) => {
                if self.elements.is_empty() {
                    return Err(4);
                }
                let text = text.into_inner();
                let position = start.walked(&raw[..raw.len().saturating_sub(3)]);
                self.characters(&text, position, &mut events);
            }
            Event::GeneralRef(reference) => {
                let name = reference.into_inner();
                if let Some(entry) = self.external.get(name.as_ref()) {
                    let data = vec![
                        self.encode(&name),
                        Value::Bool(false),
                        self.optional(entry.system_id.as_deref()),
                        self.optional(entry.public_id.as_deref()),
                    ];
                    events.push(self.emission(Handler::External, data, self.cursor));
                } else {
                    let text = self.entities.text(&name).map_err(|error| error.code)?;
                    self.characters(&text, self.cursor, &mut events);
                }
            }
            Event::PI(pi) => {
                let target = pi.target();
                let data = pi.content();
                events.push(self.emission(
                    Handler::Pi,
                    vec![self.encode(target), self.encode(data.trim_start())],
                    self.cursor,
                ));
            }
            Event::Comment(_) => {
                let comment = std::str::from_utf8(raw).map_err(|_| 9i64)?;
                events.push(self.emission(
                    Handler::Default,
                    vec![self.encode(comment)],
                    self.cursor,
                ));
            }
            Event::Decl(declaration) => {
                if self.root || start.offset != usize::from(self.bom) * 3 {
                    return Err(64);
                }
                if self.options.source.is_none()
                    && let Some(Ok(encoding)) = declaration.encoding()
                {
                    let codec = Codec::from_label(&encoding).ok_or(32i64)?;
                    if matches!(codec, Codec::Latin1) {
                        let decoded = latin1(&self.input[self.consumed..]);
                        self.input.truncate(self.consumed);
                        self.input.extend(decoded);
                    }
                    self.options.source = Some(codec);
                }
            }
            Event::DocType(doctype) => {
                if self.root {
                    return Err(5);
                }
                let text = doctype.into_inner();
                self.doctype(&text, &mut events)?;
            }
            Event::Eof => {}
        }
        if has_default {
            self.add_fallbacks(raw, &mut events);
        }
        Ok(events)
    }

    fn characters(&self, text: &str, pos: Position, events: &mut Vec<Emission>) {
        if !text.is_empty() {
            events.push(self.emission(Handler::Text, vec![self.encode(text)], pos));
        }
    }

    fn emission(&self, handler: Handler, data: Vec<Value>, position: Position) -> Emission {
        Emission {
            handler,
            data,
            position,
            fallback: None,
        }
    }

    fn encode(&self, text: &str) -> Value {
        Value::from(self.options.target.encode(text))
    }

    fn optional(&self, text: Option<&str>) -> Value {
        text.map_or(Value::Bool(false), |text| self.encode(text))
    }
}

fn latin1(data: &[u8]) -> Vec<u8> {
    data.iter()
        .copied()
        .map(char::from)
        .collect::<String>()
        .into_bytes()
}
