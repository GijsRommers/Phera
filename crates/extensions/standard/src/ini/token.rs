//! In `parse_ini_string('mask = E_ALL & ~E_NOTICE')`, words and operators are tokens.

use std::borrow::Cow;

pub(super) enum Token<'a> {
    End,
    EndLine,
    Section,
    Label(&'a [u8]),
    Offset(&'a [u8]),
    Raw(&'a [u8]),
    Constant(&'a [u8]),
    Number(&'a [u8]),
    Text(&'a [u8]),
    Space(&'a [u8]),
    Quoted(Cow<'a, [u8]>),
    Variable,
    VarName(&'a [u8]),
    Fallback,
    True,
    False,
    Null,
    Punctuation(u8),
}

impl Token<'_> {
    pub(super) fn description(&self) -> String {
        let name = match self {
            Self::End => "end of file",
            Self::EndLine => "END_OF_LINE",
            Self::Section => "TC_SECTION",
            Self::Label(_) => "TC_LABEL",
            Self::Offset(_) => "TC_OFFSET",
            Self::Raw(_) => "TC_RAW",
            Self::Constant(_) => "TC_CONSTANT",
            Self::Number(_) => "TC_NUMBER",
            Self::Text(_) => "TC_STRING",
            Self::Space(_) => "TC_WHITESPACE",
            Self::Quoted(_) => "TC_QUOTED_STRING",
            Self::Variable => "TC_DOLLAR_CURLY",
            Self::VarName(_) => "TC_VARNAME",
            Self::Fallback => "TC_FALLBACK",
            Self::True => "BOOL_TRUE",
            Self::False => "BOOL_FALSE",
            Self::Null => "NULL_NULL",
            Self::Punctuation(byte) => return format!("'{}'", char::from(*byte)),
        };
        name.into()
    }
}
