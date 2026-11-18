//! PHP serializes `[1 => "x"]` as `a:1:{i:1;s:1:"x";}`.
//!
//! The reader borrows byte strings and reports container headers separately
//! from their contents. The caller can walk them with a heap stack.

mod numbers;
mod reader;
mod writer;

pub use reader::Reader;
pub use writer::Writer;

use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectFormat {
    Properties,
    Custom,
}

#[derive(Debug, PartialEq)]
pub enum Token<'a> {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Cow<'a, [u8]>),
    Array(usize),
    Object(&'a [u8], ObjectFormat),
    Enum(&'a [u8]),
    Reference(usize),
    ObjectReference(usize),
}

#[derive(Debug, Eq, PartialEq)]
pub enum Diagnostic {
    UnexpectedEnd,
    NumberOutOfRange,
    EscapedString,
    BadObjectData,
    InsufficientCustomData {
        required: i64,
        present: usize,
        length_overflow: bool,
    },
}

#[derive(Debug, Eq, PartialEq)]
pub struct ParseError;

pub fn valid_class(name: &[u8]) -> bool {
    name.iter()
        .all(|&byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'\\' || byte >= 0x80)
}
