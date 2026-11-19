//! Read XML events into the shared DOM arena.
//!
//! ```php
//! $document->loadXML('<root><child/></root>');
//! echo $document->documentElement->firstChild->nodeName;
//! ```

mod declarations;
mod doctype;
mod dtd;
mod elements;
mod encoding;
mod entities;
pub(crate) mod names;
pub(crate) use encoding::supported_encoding;

use std::borrow::Cow;
use std::collections::HashMap;

use quick_xml::errors::IllFormedError;
use quick_xml::events::{BytesDecl, Event};
use quick_xml::{Error, Reader};

use crate::document::{Document, ParseError, Settings};
use crate::tree::{Arena, Kind, Name, Node, NodeId};

use self::encoding::{decode, xml_char};
use self::entities::Entities;

#[derive(Clone, Copy, Default)]
pub struct Options {
    pub substitute_entities: bool,
    pub strip_blanks: bool,
    pub merge_cdata: bool,
    pub default_attributes: bool,
    pub recover: bool,
    pub huge: bool,
}

pub struct Parsed {
    pub document: Option<Document>,
    pub diagnostics: Vec<ParseError>,
}

pub fn parse(input: &[u8], options: Options) -> Parsed {
    let decoded = match decode(input) {
        Ok(decoded) => decoded,
        Err(error) => {
            return Parsed {
                document: None,
                diagnostics: vec![error],
            };
        }
    };
    let input = if decoded.contains('\r') {
        Cow::Owned(decoded.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        decoded
    };
    let mut parser = Parser::new(&input, options);
    let result = match invalid_character(&input) {
        Some((offset, character)) => {
            parser.position.advance(&input, offset);
            Err(ParseError::fatal(
                9,
                format!("PCDATA invalid Char value {}", character as u32),
            ))
        }
        None => parser.run(&input),
    };
    if let Err(error) = result {
        parser.report(error);
    }
    let fatal = parser.diagnostics.iter().any(|error| error.level == 3);
    let limit = parser.diagnostics.iter().any(|error| error.code == 89);
    let document =
        (!fatal || (options.recover && parser.root && !limit)).then_some(parser.document);
    Parsed {
        document,
        diagnostics: parser.diagnostics,
    }
}

fn invalid_character(input: &str) -> Option<(usize, char)> {
    let offset = input
        .bytes()
        .position(|byte| matches!(byte, 0..=8 | 11..=12 | 14..=31 | 128..=255))?;
    input[offset..]
        .char_indices()
        .find(|&(_, character)| !xml_char(character))
        .map(|(index, character)| (offset + index, character))
}

type ScopeChanges = Vec<(String, Option<String>)>;

struct Parser<'a> {
    reader: Reader<&'a [u8]>,
    document: Document,
    open: Vec<(NodeId, ScopeChanges)>,
    scope: HashMap<String, String>,
    entities: Entities,
    attribute_rules: dtd::AttributeRules,
    diagnostics: Vec<ParseError>,
    position: Position,
    root: bool,
    doctype: bool,
    options: Options,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str, options: Options) -> Self {
        let mut reader = Reader::from_str(input);
        reader.config_mut().check_comments = true;
        Self {
            reader,
            document: Document::default(),
            open: Vec::new(),
            scope: HashMap::from([("xml".into(), names::XML.into())]),
            entities: Entities::default(),
            attribute_rules: dtd::AttributeRules::default(),
            diagnostics: Vec::new(),
            position: Position::default(),
            root: false,
            doctype: false,
            options,
        }
    }

    fn run(&mut self, input: &str) -> Result<(), ParseError> {
        loop {
            let start = self.reader.buffer_position() as usize;
            self.position.advance(input, start);
            let event = self.reader.read_event();
            let end = self.reader.buffer_position() as usize;
            let line = self.position.line;
            let at_start = match &event {
                Ok(Event::Start(_) | Event::Empty(_)) if self.open.is_empty() && self.root => true,
                Ok(Event::Text(text)) if self.open.is_empty() => !blank(text),
                _ => false,
            };
            if !at_start {
                let delimiter = match &event {
                    Ok(Event::Empty(_)) => 2,
                    Ok(Event::Start(_)) => 1,
                    _ => 0,
                };
                self.position.advance(input, end.saturating_sub(delimiter));
            }
            match event {
                Ok(Event::Start(tag)) => self.element(tag, line, true)?,
                Ok(Event::Empty(tag)) => self.element(tag, line, false)?,
                Ok(Event::End(_)) => self.close(),
                Ok(Event::Text(text)) => self.text(&text, Kind::Text)?,
                Ok(Event::CData(data)) => {
                    let kind = if self.options.merge_cdata {
                        Kind::Text
                    } else {
                        Kind::Cdata
                    };
                    self.text(&data, kind)?;
                }
                Ok(Event::Comment(data)) => {
                    self.add(Kind::Comment, "#comment", &data, line)?;
                }
                Ok(Event::PI(data)) => {
                    let target = data.target();
                    if target.eq_ignore_ascii_case("xml") {
                        return Err(ParseError::fatal(
                            64,
                            "XML declaration allowed only at the start",
                        ));
                    }
                    self.add(
                        Kind::ProcessingInstruction,
                        target,
                        data.content().trim_start_matches(xml_space),
                        line,
                    )?;
                }
                Ok(Event::Decl(decl)) if start == 0 => self.declaration(decl)?,
                Ok(Event::Decl(_)) => {
                    return Err(ParseError::fatal(
                        64,
                        "XML declaration allowed only at the start",
                    ));
                }
                Ok(Event::DocType(data)) if !self.root && !self.doctype => {
                    self.add_doctype(&data, line)?;
                }
                Ok(Event::DocType(_)) => return Err(extra_content()),
                Ok(Event::GeneralRef(reference)) if !self.open.is_empty() => {
                    self.reference(&reference, line)?;
                }
                Ok(Event::GeneralRef(_)) => return Err(self.unexpected_text()),
                Ok(Event::Eof) => return self.finish(),
                Err(error @ Error::IllFormed(IllFormedError::MismatchedEndTag { .. })) => {
                    let error = self.reader_error(error);
                    self.report(error);
                    self.close();
                }
                Err(error) => return Err(self.reader_error(error)),
            }
        }
    }

    fn reference(&mut self, name: &str, line: usize) -> Result<(), ParseError> {
        match self.entities.resolve(name) {
            Ok((value, true)) if !self.options.substitute_entities => {
                self.document.arena.materialize_entity(name, &value);
                self.add(Kind::EntityReference, name, &value, line)?;
            }
            Ok((value, _)) => self.text(&value, Kind::Text)?,
            Err(error) if error.code == 26 => self.report(error),
            Err(error) => return Err(error),
        }
        Ok(())
    }

    fn finish(&self) -> Result<(), ParseError> {
        match self.open.last() {
            Some(&(id, _)) => {
                let node = &self.document.arena[id];
                Err(ParseError::fatal(
                    77,
                    format!(
                        "Premature end of data in tag {} line {}",
                        node.name.qualified(),
                        node.line
                    ),
                ))
            }
            None if !self.root => Err(ParseError::fatal(4, "Document is empty")),
            None => Ok(()),
        }
    }

    fn unexpected_text(&self) -> ParseError {
        if self.root {
            extra_content()
        } else {
            ParseError::fatal(4, "Start tag expected, '<' not found")
        }
    }

    fn declaration(&mut self, declaration: BytesDecl<'_>) -> Result<(), ParseError> {
        let version = declaration
            .version()
            .map_err(|error| self.reader_error(error))?;

        if version != "1.0" {
            let mut warning = ParseError::fatal(97, format!("Unsupported version '{version}'"));
            warning.level = 1;
            self.report(warning);
        }
        let encoding = declaration.encoding().and_then(Result::ok);
        let standalone = declaration.standalone().and_then(Result::ok);
        self.document.settings = Settings {
            version: Some(version.into_owned()),
            encoding: encoding.map(Cow::into_owned),
            standalone: standalone.as_deref().map(|value| value == "yes"),
            ..Settings::default()
        };
        Ok(())
    }

    fn parent(&self) -> NodeId {
        self.open.last().map_or(Arena::DOCUMENT, |frame| frame.0)
    }

    fn add(
        &mut self,
        kind: Kind,
        name: &str,
        value: &str,
        line: usize,
    ) -> Result<NodeId, ParseError> {
        let mut node = Node::new(kind, Name::new(name), value);
        node.line = line;
        let parent = self.parent();
        let id = self.document.arena.allocate(node);
        self.document
            .arena
            .insert(parent, id, None)
            .map_err(|_| self.unexpected_text())?;
        Ok(id)
    }

    fn text(&mut self, text: &str, kind: Kind) -> Result<(), ParseError> {
        if text.is_empty() && kind == Kind::Text {
            return Ok(());
        }
        if self.open.is_empty() {
            return if kind == Kind::Text && blank(text) {
                Ok(())
            } else {
                Err(self.unexpected_text())
            };
        }
        let parent = self.parent();
        let arena = &mut self.document.arena;
        if kind == Kind::Text
            && let Some(last) = arena[parent].last_child()
            && arena[last].kind == Kind::Text
        {
            arena[last].value.push_str(text);
        } else if kind != Kind::Text || !self.options.strip_blanks || !blank(text) {
            let name = if kind == Kind::Cdata {
                "#cdata-section"
            } else {
                "#text"
            };
            self.add(kind, name, text, self.position.line)?;
        }
        Ok(())
    }

    fn report(&mut self, mut error: ParseError) {
        error.line = self.position.line;
        error.column = self.position.column;
        error.message.push('\n');
        self.diagnostics.push(error);
    }

    fn reader_error(&self, error: Error) -> ParseError {
        match error {
            Error::IllFormed(IllFormedError::MismatchedEndTag { expected, found }) => {
                let line = self.document.arena[self.parent()].line;
                ParseError::fatal(
                    76,
                    format!("Opening and ending tag mismatch: {expected} line {line} and {found}"),
                )
            }
            Error::IllFormed(IllFormedError::UnclosedReference) => {
                ParseError::fatal(23, "EntityRef: expecting ';'")
            }
            other => ParseError::fatal(73, other.to_string()),
        }
    }
}

struct Position {
    offset: usize,
    line: usize,
    column: usize,
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
    fn advance(&mut self, input: &str, offset: usize) {
        if self.offset == offset {
            return;
        }
        if let Some(chunk) = input.get(self.offset..offset) {
            if let Some((complete, tail)) = chunk.rsplit_once('\n') {
                self.line += 1 + complete.bytes().filter(|&byte| byte == b'\n').count();
                self.column = 1 + tail.chars().count();
            } else {
                self.column += chunk.chars().count();
            }
            self.offset = offset;
        }
    }
}

fn blank(text: &str) -> bool {
    text.chars().all(xml_space)
}

fn xml_space(character: char) -> bool {
    matches!(character, ' ' | '\n' | '\r' | '\t')
}

fn extra_content() -> ParseError {
    ParseError::fatal(5, "Extra content at the end of the document")
}
