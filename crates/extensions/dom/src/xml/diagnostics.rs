//! Track XML source positions and translate parser errors to libxml diagnostics.
//!
//! ```php
//! libxml_use_internal_errors(true);
//! $document->loadXML('<root>');
//! echo libxml_get_last_error()->line;
//! ```

use quick_xml::Error;
use quick_xml::errors::IllFormedError;

use crate::document::ParseError;

use super::Parser;

impl Parser<'_> {
    pub(super) fn finish(&self) -> Result<(), ParseError> {
        match self.open.last() {
            Some(frame) => {
                let node = &self.document.arena[frame.id];
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

    pub(super) fn report(&mut self, mut error: ParseError) {
        error.line = self.position.line;
        error.column = self.position.column;
        error.message.push('\n');
        self.diagnostics.push(error);
    }

    pub(super) fn reader_error(&self, error: Error) -> ParseError {
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

pub(super) struct Position {
    offset: usize,
    pub(super) line: usize,
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
    pub(super) fn advance(&mut self, input: &str, offset: usize) {
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
