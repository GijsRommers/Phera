//! HTML/PHP tag stripping, ported from php-src ext/standard/string.c, php_strip_tags_ex.
//! Allowed tags preserve their original attributes and spelling.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

use super::{buffer, bytes};

mod allowlist;

use allowlist::{allowed_tags, permitted};

const STRIP_TAGS_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("allowed_tags", ParamType::ArrayOrString)
        .nullable()
        .optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "strip_tags",
        STRIP_TAGS_PARAMS,
        strip_tags,
    ));
}

/// Nested brackets, quotes, processing instructions, declarations and comments use
/// separate states, matching php_strip_tags_ex rather than an HTML parser.
fn strip_tags(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 0);
    let allowed = allowed_tags(context, arguments)?;
    let mut parser = TagStripper {
        input,
        allowed: &allowed,
        output: buffer(input.len())?,
        tag: buffer(input.len())?,
        state: State::Text,
        quote: None,
        significant: None,
        depth: 0,
        brackets: 0,
        kind: TagKind::Html,
    };
    for (cursor, &byte) in input.iter().enumerate() {
        parser.consume(cursor, byte);
    }
    Ok(bytes(parser.output))
}

#[derive(Clone, Copy)]
enum State {
    Text,
    Tag,
    ProcessingInstruction,
    Declaration,
    Comment,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TagKind {
    Html,
    Xml,
}

struct TagStripper<'a> {
    input: &'a [u8],
    allowed: &'a [u8],
    output: Vec<u8>,
    tag: Vec<u8>,
    state: State,
    quote: Option<u8>,
    significant: Option<u8>,
    depth: usize,
    brackets: i64,
    kind: TagKind,
}

impl TagStripper<'_> {
    /// Dispatch by PHP's lexical state, rather than interpreting HTML structure.
    fn consume(&mut self, cursor: usize, byte: u8) {
        let previous = cursor
            .checked_sub(1)
            .and_then(|index| self.input.get(index))
            .copied()
            .unwrap_or_default();
        match self.state {
            State::Text => self.text(cursor, byte),
            State::Tag => self.tag(cursor, byte, previous),
            State::ProcessingInstruction => self.processing_instruction(cursor, byte, previous),
            State::Declaration => self.declaration(cursor, byte, previous),
            State::Comment => self.comment(cursor, byte),
        }
    }

    /// Whitespace after an opening bracket keeps it literal in PHP.
    fn text(&mut self, cursor: usize, byte: u8) {
        match byte {
            0 => {}
            b'<' if self.quote.is_none() => {
                if self
                    .input
                    .get(cursor + 1)
                    .is_some_and(u8::is_ascii_whitespace)
                {
                    self.output.push(byte);
                } else {
                    self.significant = Some(b'<');
                    self.state = State::Tag;
                    if !self.allowed.is_empty() {
                        self.tag.push(byte);
                    }
                }
            }
            b'<' => {}
            b'>' if self.depth > 0 => {
                self.depth -= 1;
            }
            b'>' if self.quote.is_some() => {}
            _ => self.output.push(byte),
        }
    }

    /// Quotes and nested brackets delay tag termination.
    fn tag(&mut self, cursor: usize, byte: u8, previous: u8) {
        let mut regular = false;
        match byte {
            0 => {}
            b'<' => {
                if self.quote.is_none() {
                    if self
                        .input
                        .get(cursor + 1)
                        .is_some_and(u8::is_ascii_whitespace)
                    {
                        regular = true;
                    } else {
                        self.depth += 1;
                    }
                }
            }
            b'>' => self.close_tag(previous),
            b'\'' | b'"' => {
                if self.quote.is_none() || self.quote == Some(byte) {
                    self.quote = if self.quote.is_none() {
                        Some(byte)
                    } else {
                        None
                    };
                }
                regular = true;
            }
            b'!' if previous == b'<' => {
                self.state = State::Declaration;
                self.significant = Some(byte);
            }
            b'?' if previous == b'<' => {
                self.brackets = 0;
                self.state = State::ProcessingInstruction;
            }
            _ => regular = true,
        }
        if regular && !self.allowed.is_empty() {
            self.tag.push(byte);
        }
    }

    /// XML-like tags retain PHP's special handling of a hyphen before the close.
    fn close_tag(&mut self, previous: u8) {
        if self.depth > 0 {
            self.depth -= 1;
        } else if self.quote.is_none() {
            self.significant = Some(b'>');
            if !(self.kind == TagKind::Xml && previous == b'-') {
                self.quote = None;
                self.state = State::Text;
                self.kind = TagKind::Html;
                if !self.allowed.is_empty() {
                    self.tag.push(b'>');
                    if permitted(&self.tag, self.allowed) {
                        self.output.extend_from_slice(&self.tag);
                    }
                    self.tag.clear();
                }
            }
        }
    }

    /// PHP processing instructions close only outside quotes and parentheses.
    fn processing_instruction(&mut self, cursor: usize, byte: u8, previous: u8) {
        match byte {
            b'(' if !matches!(self.significant, Some(b'"' | b'\'')) => {
                self.significant = Some(byte);
                self.brackets = self.brackets.saturating_add(1);
            }
            b')' if !matches!(self.significant, Some(b'"' | b'\'')) => {
                self.significant = Some(byte);
                self.brackets = self.brackets.saturating_sub(1);
            }
            b'>' if self.depth > 0 => {
                self.depth -= 1;
            }
            b'>' if self.quote.is_none()
                && self.brackets == 0
                && self.significant != Some(b'"')
                && previous == b'?' =>
            {
                self.quote = None;
                self.state = State::Text;
                self.tag.clear();
            }
            b'\'' | b'"' if previous != b'\\' => {
                if self.significant == Some(byte) {
                    self.significant = None;
                } else if self.significant != Some(b'\\') {
                    self.significant = Some(byte);
                }
                if self.quote.is_none() || self.quote == Some(byte) {
                    self.quote = if self.quote.is_none() {
                        Some(byte)
                    } else {
                        None
                    };
                }
            }
            b'l' | b'L'
                if cursor > 4
                    && self
                        .input
                        .get(cursor - 4..=cursor)
                        .is_some_and(|part| part.eq_ignore_ascii_case(b"<?xml")) =>
            {
                self.state = State::Tag;
                self.kind = TagKind::Xml;
            }
            _ => {}
        }
    }

    /// Declarations can transition into comments or ordinary doctype tags.
    fn declaration(&mut self, cursor: usize, byte: u8, previous: u8) {
        match byte {
            b'>' if self.depth > 0 => {
                self.depth -= 1;
            }
            b'>' if self.quote.is_none() => {
                self.quote = None;
                self.state = State::Text;
                self.tag.clear();
            }
            b'\'' | b'"'
                if previous != b'\\' && (self.quote.is_none() || self.quote == Some(byte)) =>
            {
                self.quote = if self.quote.is_none() {
                    Some(byte)
                } else {
                    None
                };
            }
            b'-' if cursor >= 2 && self.input.get(cursor - 2..cursor) == Some(b"!-".as_slice()) => {
                self.state = State::Comment
            }
            b'e' | b'E'
                if cursor > 6
                    && self
                        .input
                        .get(cursor - 6..=cursor)
                        .is_some_and(|part| part.eq_ignore_ascii_case(b"doctype")) =>
            {
                self.state = State::Tag
            }
            _ => {}
        }
    }

    /// Comments require a literal closing double hyphen.
    fn comment(&mut self, cursor: usize, byte: u8) {
        if byte == b'>'
            && self.quote.is_none()
            && cursor >= 2
            && self.input.get(cursor - 2..cursor) == Some(b"--".as_slice())
        {
            self.quote = None;
            self.state = State::Text;
            self.tag.clear();
        }
    }
}
