//! XMLWriter's output and open constructs.
//!
//! ```php
//! $writer = XMLWriter::toMemory();
//! $writer->startElement('root');
//! echo $writer->flush(false); // <root
//! $writer->endElement();
//! echo $writer->flush(); // <root/>
//! ```

use crate::{bytes, dtd};

#[derive(Clone, Default)]
pub struct Writer {
    pub output: Vec<u8>,
    pub encoding: bytes::Encoding,
    pub indent: bool,
    pub padding: Vec<u8>,
    pub frames: Vec<Frame>,
    level: usize,
    indent_close: bool,
}

#[derive(Clone)]
pub enum Frame {
    Element(Element),
    Attribute,
    Comment,
    Cdata,
    Pi {
        started: bool,
    },
    Doctype {
        subset: bool,
    },
    Declaration {
        kind: dtd::Declaration,
        started: bool,
    },
}

#[derive(Clone)]
pub struct Element {
    name: Vec<u8>,
    open: bool,
    namespaces: Vec<(Option<Vec<u8>>, Vec<u8>)>,
}

impl Writer {
    pub fn new() -> Self {
        Self {
            padding: b" ".to_vec(),
            indent_close: true,
            ..Self::default()
        }
    }

    pub fn padding_at(&mut self, depth: usize) {
        if self.indent {
            for _ in 0..depth {
                self.output.extend_from_slice(&self.padding);
            }
        }
    }

    pub fn newline(&mut self) {
        if self.indent {
            self.output.push(b'\n');
        }
    }

    fn finish_tag(&mut self, suffix: &[u8]) {
        let Some(Frame::Element(element)) = self.frames.last_mut() else {
            return;
        };
        if !element.open {
            return;
        };
        for (prefix, namespace) in element.namespaces.iter().rev() {
            self.output.extend_from_slice(b" xmlns");
            if let Some(prefix) = prefix {
                self.output.push(b':');
                self.output.extend_from_slice(prefix);
            }
            self.output.extend_from_slice(b"=\"");
            bytes::escaped(&mut self.output, namespace, true);
            self.output.push(b'"');
        }
        self.output.extend_from_slice(suffix);
        element.open = false;
    }

    pub fn start_element(
        &mut self,
        prefix: Option<&[u8]>,
        name: &[u8],
        namespace: Option<&[u8]>,
    ) -> bool {
        if matches!(self.frames.last(), Some(Frame::Pi { .. })) {
            return false;
        }
        self.end_attribute();
        self.finish_tag(if self.indent { b">\n" } else { b">" });
        self.padding_at(self.level);
        self.level += 1;
        self.indent_close = true;
        let name = bytes::qualified(prefix, name);
        self.output.push(b'<');
        self.output.extend_from_slice(&name);
        let namespaces = namespace
            .map(|uri| (prefix.map(<[u8]>::to_vec), uri.to_vec()))
            .into_iter()
            .collect();
        self.frames.push(Frame::Element(Element {
            name,
            open: true,
            namespaces,
        }));
        true
    }

    pub fn end_element(&mut self, full: bool) -> bool {
        self.end_attribute();
        let Some(Frame::Element(element)) = self.frames.last() else {
            return false;
        };
        self.level -= 1;
        let empty = element.open;
        self.finish_tag(if full { b">" } else { b"/>" });
        let Some(Frame::Element(element)) = self.frames.pop() else {
            return false;
        };
        if !empty || full {
            if !empty && self.indent_close {
                self.padding_at(self.level);
            }
            self.output.extend_from_slice(b"</");
            self.output.extend_from_slice(&element.name);
            self.output.push(b'>');
        }
        self.newline();
        self.indent_close = true;
        true
    }

    pub fn start_attribute(
        &mut self,
        prefix: Option<&[u8]>,
        name: &[u8],
        namespace: Option<&[u8]>,
    ) -> bool {
        self.end_attribute();
        let Some(Frame::Element(Element {
            open: true,
            namespaces,
            ..
        })) = self.frames.last_mut()
        else {
            return false;
        };
        if let Some(uri) = namespace {
            match namespaces.iter().find(|(p, _)| p.as_deref() == prefix) {
                Some((_, existing)) if existing != uri => return false,
                Some(_) => {}
                None => namespaces.push((prefix.map(<[u8]>::to_vec), uri.to_vec())),
            }
        }
        self.output.push(b' ');
        self.output.extend(bytes::qualified(prefix, name));
        self.output.extend_from_slice(b"=\"");
        self.frames.push(Frame::Attribute);
        true
    }

    pub fn end_attribute(&mut self) -> bool {
        if !matches!(self.frames.last(), Some(Frame::Attribute)) {
            return false;
        }
        self.frames.pop();
        self.output.push(b'"');
        true
    }

    pub fn start_section(&mut self, section: Frame, name: &[u8]) -> bool {
        self.end_attribute();
        if !matches!(self.frames.last(), Some(Frame::Element(_)) | None) {
            return false;
        }
        let opening: &[u8] = match section {
            Frame::Comment => {
                self.finish_tag(if self.indent { b">\n" } else { b">" });
                self.padding_at(self.level);
                b"<!--"
            }
            Frame::Pi { .. } => {
                self.finish_tag(b">");
                b"<?"
            }
            Frame::Cdata => {
                self.finish_tag(b">");
                self.indent_close = false;
                b"<![CDATA["
            }
            _ => return false,
        };
        self.output.extend_from_slice(opening);
        self.output.extend_from_slice(name);
        self.frames.push(section);
        true
    }

    pub fn end_section(&mut self, section: Frame) -> bool {
        let (suffix, indent): (&[u8], bool) = match (self.frames.last(), section) {
            (Some(Frame::Comment), Frame::Comment) => (b"-->", true),
            (Some(Frame::Pi { .. }), Frame::Pi { .. }) => (b"?>", true),
            (Some(Frame::Cdata), Frame::Cdata) => (b"]]>", false),
            _ => return false,
        };
        self.frames.pop();
        self.output.extend_from_slice(suffix);
        if indent {
            self.indent_close = true;
            self.newline();
        }
        true
    }

    pub fn text(&mut self, value: &[u8], raw: bool) {
        let value = bytes::nul_prefix(value);
        match self.frames.last_mut() {
            Some(Frame::Pi { started }) if !*started => {
                self.output.push(b' ');
                *started = true;
            }
            Some(Frame::Doctype { .. } | Frame::Declaration { .. }) => dtd::text_prefix(self),
            Some(Frame::Element(_)) => {
                self.finish_tag(b">");
                self.indent_close = false;
            }
            _ => {}
        }
        let attribute = matches!(self.frames.last(), Some(Frame::Attribute));
        if raw || (!attribute && !matches!(self.frames.last(), None | Some(Frame::Element(_)))) {
            self.output.extend_from_slice(value);
        } else {
            bytes::escaped(&mut self.output, value, attribute);
        }
    }

    pub fn start_document(
        &mut self,
        version: &[u8],
        encoding: Option<&[u8]>,
        standalone: Option<&[u8]>,
    ) -> Result<bool, &'static str> {
        if !self.frames.is_empty() {
            return Err("xmlTextWriterStartDocument : not allowed in this context!");
        }
        self.encoding = bytes::Encoding::from_label(encoding)?;
        self.output.extend_from_slice(b"<?xml version=\"");
        self.output.extend_from_slice(version);
        self.output.push(b'"');
        if let Some(encoding) = encoding {
            self.output.extend_from_slice(b" encoding=\"");
            self.output.extend_from_slice(encoding);
            self.output.push(b'"');
        }
        if let Some(standalone) = standalone {
            self.output.extend_from_slice(b" standalone=\"");
            self.output.extend_from_slice(standalone);
            self.output.push(b'"');
        }
        self.output.extend_from_slice(b"?>\n");
        Ok(true)
    }

    pub fn end_document(&mut self) -> bool {
        while let Some(frame) = self.frames.last() {
            match frame {
                Frame::Element(_) => self.end_element(false),
                Frame::Attribute => self.end_attribute(),
                Frame::Comment => self.end_section(Frame::Comment),
                Frame::Pi { .. } => self.end_section(Frame::Pi { started: false }),
                Frame::Cdata => self.end_section(Frame::Cdata),
                Frame::Doctype { .. } => dtd::end(self),
                Frame::Declaration { kind, .. } => dtd::end_declaration(self, *kind),
            };
        }
        if !self.indent {
            self.output.push(b'\n');
        }
        true
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        self.encoding.convert(std::mem::take(&mut self.output))
    }
}
