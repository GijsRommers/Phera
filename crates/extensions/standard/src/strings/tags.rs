//! `strip_tags("<b>bold</b> text")` is `"bold text"`, ported from php-src
//! ext/standard/string.c, `php_strip_tags_ex`.
//!
//! Like PHP, this is a small state machine over bytes, not an HTML parser:
//! it tracks quotes, nested `<`, comments, `<?php … ?>` and `<!…>`, and gets
//! the same answers as PHP for broken markup too.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

use super::buffer;

mod allowlist;

use allowlist::{allowed_tags, permitted};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[STRIP_TAGS];

/// `strip_tags(string $string, array|string|null $allowed_tags = null):
/// string`, the text without HTML and PHP tags, keeping the allowed ones
/// exactly as written, attributes and all.
///
/// ```php
/// strip_tags("<p>Hi <b>you</b></p><!-- note -->");    // "Hi you"
/// strip_tags("<p>Hi <b class=x>you</b></p>", "<b>");  // "Hi <b class=x>you</b>"
/// strip_tags("<p>Hi <b>you</b></p>", ["p"]);          // "<p>Hi you</p>"
/// ```
const STRIP_TAGS: FunctionEntry = FunctionEntry::sync(
    "strip_tags",
    &[
        Param::new("string", ParamType::String),
        Param::new("allowed_tags", ParamType::ArrayOrString)
            .nullable()
            .optional(),
    ],
    strip_tags,
);

fn strip_tags(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 0);
    let allowed = allowed_tags(context, arguments)?;
    let mut stripper = TagStripper {
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
        stripper.consume(cursor, byte);
    }
    Ok(Value::from(stripper.output))
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

/// The state of `php_strip_tags_ex`, with the C names in brackets.
struct TagStripper<'a> {
    input: &'a [u8],
    /// The allow-list as `<a><b>`, lower case; empty for none.
    allowed: &'a [u8],
    output: Vec<u8>,
    /// The tag being read (`tbuf`), kept only when there is an allow-list.
    tag: Vec<u8>,
    state: State,
    /// The quote the scanner is inside of (`in_q`).
    quote: Option<u8>,
    /// The last byte that changed the state (`lc`).
    significant: Option<u8>,
    /// How many `<` inside a tag are still open (`depth`).
    depth: usize,
    /// Open parentheses inside `<?php … ?>` (`br`).
    brackets: i64,
    kind: TagKind,
}

impl TagStripper<'_> {
    /// Dispatch by PHP's lexical state, rather than interpreting HTML structure.
    fn consume(&mut self, cursor: usize, byte: u8) {
        // Before the first byte, PHP's `lc` is NUL.
        let previous = cursor.checked_sub(1).map_or(0, |index| self.input[index]);
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
