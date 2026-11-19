//! XML names and namespace declarations.
//!
//! ```php
//! $node = $document->createElementNS('urn:example', 'p:root');
//! ```

use crate::tree::Name;

pub(crate) const XML: &str = "http://www.w3.org/XML/1998/namespace";
pub(crate) const XMLNS: &str = "http://www.w3.org/2000/xmlns/";

pub(crate) fn valid(name: &str) -> bool {
    let mut characters = name.chars();
    characters.next().is_some_and(start) && characters.all(rest)
}

fn start(character: char) -> bool {
    matches!(
        character,
        ':' | 'A'..='Z' | '_' | 'a'..='z' | '\u{c0}'..='\u{d6}' | '\u{d8}'..='\u{f6}'
            | '\u{f8}'..='\u{2ff}' | '\u{370}'..='\u{37d}' | '\u{37f}'..='\u{1fff}'
            | '\u{200c}'..='\u{200d}' | '\u{2070}'..='\u{218f}' | '\u{2c00}'..='\u{2fef}'
            | '\u{3001}'..='\u{d7ff}' | '\u{f900}'..='\u{fdcf}' | '\u{fdf0}'..='\u{fffd}'
            | '\u{10000}'..='\u{effff}'
    )
}

fn rest(character: char) -> bool {
    start(character)
        || matches!(character, '-' | '.' | '0'..='9' | '\u{b7}' |
            '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}')
}

pub(crate) fn qualified(name: &str, namespace: Option<String>) -> Name {
    match name.split_once(':') {
        Some((prefix, local)) => Name {
            local: local.into(),
            prefix: Some(prefix.into()),
            namespace,
        },
        None => Name {
            local: name.into(),
            prefix: None,
            namespace,
        },
    }
}
