//! Parse CSS selector lists with the namespaces visible at the query root.
//!
//! ```php
//! $root->querySelector('p|item, article:has(a[href])');
//! ```

use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt::{self, Write};
use std::hash::{DefaultHasher, Hash, Hasher};

use cssparser::{CowRcStr, ParseError, Parser as CssParser, ToCss};
use precomputed_hash::PrecomputedHash;
use selectors::parser::{NonTSPseudoClass, PseudoElement, SelectorParseErrorKind};
use selectors::{Parser, SelectorImpl};

use crate::bindings::{dom_error, node};
use crate::tree::{Arena, NodeId};

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(super) struct Atom(pub String);

impl From<&str> for Atom {
    fn from(text: &str) -> Self {
        Self(text.to_owned())
    }
}

impl Borrow<str> for Atom {
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<str> for Atom {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl ToCss for Atom {
    fn to_css<W: Write>(&self, output: &mut W) -> fmt::Result {
        cssparser::serialize_identifier(&self.0, output)
    }
}

impl PrecomputedHash for Atom {
    fn precomputed_hash(&self) -> u32 {
        let mut hash = DefaultHasher::new();
        self.0.hash(&mut hash);
        hash.finish() as u32
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Pseudo(pub String, pub Option<String>);

impl NonTSPseudoClass for Pseudo {
    fn is_active_or_hover(&self) -> bool {
        matches!(self.0.as_str(), "active" | "hover")
    }
    fn is_user_action_state(&self) -> bool {
        self.is_active_or_hover()
            || matches!(self.0.as_str(), "focus" | "focus-visible" | "focus-within")
    }
}

impl ToCss for Pseudo {
    fn to_css<W: Write>(&self, output: &mut W) -> fmt::Result {
        write!(output, ":{}", self.0)?;
        if let Some(argument) = &self.1 {
            write!(output, "({argument})")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum NoPseudoElement {}

impl ToCss for NoPseudoElement {
    fn to_css<W: Write>(&self, _: &mut W) -> fmt::Result {
        match *self {}
    }
}

impl PseudoElement for NoPseudoElement {}

#[derive(Clone, Debug)]
pub(super) struct Css;

impl SelectorImpl for Css {
    type ExtraMatchingData<'a> = ();
    type AttrValue = Atom;
    type Identifier = Atom;
    type LocalName = Atom;
    type NamespaceUrl = Atom;
    type NamespacePrefix = Atom;
    type BorrowedNamespaceUrl = str;
    type BorrowedLocalName = str;
    type NonTSPseudoClass = Pseudo;
    type PseudoElement = NoPseudoElement;
}

pub(super) struct Namespaces(HashMap<String, Atom>);

impl Namespaces {
    pub(super) fn new(arena: &Arena, id: NodeId) -> Self {
        let mut namespaces = HashMap::new();
        for (name, uri) in node::namespace_scope(arena, id) {
            namespaces.entry(name).or_insert(Atom(uri));
        }
        Self(namespaces)
    }
}

impl<'i> Parser<'i> for Namespaces {
    type Impl = Css;
    type Error = SelectorParseErrorKind;

    fn parse_is_and_where(&self) -> bool {
        true
    }
    fn parse_has(&self) -> bool {
        true
    }
    fn parse_nth_child_of(&self) -> bool {
        true
    }

    fn namespace_for_prefix(&self, prefix: &Atom) -> Option<Atom> {
        Some(
            self.0
                .get(&prefix.0)
                .cloned()
                .unwrap_or_else(|| Atom(format!("urn:php:unbound-prefix:{}", prefix.0))),
        )
    }

    fn parse_non_ts_pseudo_class(
        &self,
        name: CowRcStr<'i>,
    ) -> Result<Pseudo, ParseError<Self::Error>> {
        let name = name.to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "link"
                | "any-link"
                | "checked"
                | "enabled"
                | "disabled"
                | "indeterminate"
                | "valid"
                | "invalid"
                | "required"
                | "optional"
                | "read-only"
                | "read-write"
                | "placeholder-shown"
                | "focus"
                | "focus-within"
                | "focus-visible"
                | "hover"
                | "active"
                | "visited"
                | "target"
                | "defined"
                | "default"
        ) {
            Ok(Pseudo(name, None))
        } else {
            Err(ParseError::custom(
                SelectorParseErrorKind::UnsupportedPseudoClassOrElement,
            ))
        }
    }

    fn parse_non_ts_functional_pseudo_class(
        &self,
        name: CowRcStr<'i>,
        input: &mut CssParser<'i>,
        _: bool,
    ) -> Result<Pseudo, ParseError<Self::Error>> {
        if name.eq_ignore_ascii_case("lang") || name.eq_ignore_ascii_case("dir") {
            let language = input.expect_ident_or_string()?;
            return Ok(Pseudo(
                name.to_ascii_lowercase(),
                Some(language.to_string()),
            ));
        }
        Err(ParseError::custom(
            SelectorParseErrorKind::UnsupportedPseudoClassOrElement,
        ))
    }
}

pub(super) fn error(
    source: &str,
    error: &ParseError<SelectorParseErrorKind>,
) -> phera_extension_api::NativeError {
    let last = source.split(':').next_back().unwrap_or_default().trim();
    let reason = if source.contains("::") && !last.is_empty() {
        format!("Not supported: {last}")
    } else if matches!(
        error.kind,
        cssparser::ParseErrorKind::Custom(SelectorParseErrorKind::UnsupportedPseudoClassOrElement)
    ) && !last.is_empty()
    {
        format!("Unexpected token: {last}")
    } else {
        "Unexpected token: END-OF-FILE".into()
    };
    dom_error(12, format!("Invalid selector (Selectors. {reason})"))
}

pub(super) fn bounded(source: &str) -> bool {
    // A selector can recurse over its own expressions, independently of the tree depth.
    if source.len() > 16_384
        || source
            .bytes()
            .filter(|&byte| {
                matches!(
                    byte,
                    b'>' | b'+' | b'~' | b' ' | b'\t' | b'\r' | b'\n' | b'\x0c'
                )
            })
            .count()
            > 512
    {
        return false;
    }
    let mut input = source.bytes().peekable();
    let mut brackets = Vec::new();
    let mut quote = None;
    while let Some(byte) = input.next() {
        if byte == b'\\' {
            input.next();
            continue;
        }
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            continue;
        }
        match byte {
            b'"' | b'\'' => quote = Some(byte),
            b'/' if input.next_if_eq(&b'*').is_some() => {
                while let Some(comment) = input.next() {
                    if comment == b'*' && input.next_if_eq(&b'/').is_some() {
                        break;
                    }
                }
            }
            b'(' | b'[' | b'{' => {
                brackets.push(byte);
                if brackets.len() > 64 {
                    return false;
                }
            }
            b')' if brackets.last() == Some(&b'(') => {
                brackets.pop();
            }
            b']' if brackets.last() == Some(&b'[') => {
                brackets.pop();
            }
            b'}' if brackets.last() == Some(&b'{') => {
                brackets.pop();
            }
            _ => {}
        }
    }
    true
}
