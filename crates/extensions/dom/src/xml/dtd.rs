//! Track ID and default attributes declared in the internal DTD subset.
//!
//! ```php
//! $document->loadXML('<!DOCTYPE r [<!ATTLIST r key ID #IMPLIED>]><r key="a"/>');
//! echo $document->getElementById('a')->nodeName;
//! ```

use std::collections::HashMap;

use super::entities::quoted;
use super::xml_space;

#[derive(Default)]
pub(super) struct AttributeRules(HashMap<String, Vec<AttributeRule>>);

pub(super) struct AttributeRule {
    pub name: String,
    pub is_id: bool,
    pub default: Option<String>,
}

impl AttributeRules {
    pub(super) fn declare(&mut self, data: &str) {
        let mut tokens = Tokens(data);
        let Some((element, false)) = tokens.next() else {
            return;
        };
        let rules = self.0.entry(element.into()).or_default();
        while let Some((name, false)) = tokens.next() {
            let Some((kind, false)) = tokens.next() else {
                break;
            };
            if kind == "NOTATION" && tokens.next().is_none() {
                break;
            }
            let Some((value, quoted)) = tokens.next() else {
                break;
            };
            let default = match (value, quoted) {
                (_, true) => Some(value.into()),
                ("#FIXED", false) => tokens
                    .next()
                    .filter(|&(_, quoted)| quoted)
                    .map(|(value, _)| value.into()),
                _ => None,
            };
            if !rules.iter().any(|rule| rule.name == name) {
                rules.push(AttributeRule {
                    name: name.into(),
                    is_id: kind == "ID",
                    default,
                });
            }
        }
    }

    pub(super) fn get(&self, element: &str) -> &[AttributeRule] {
        self.0.get(element).map_or(&[], Vec::as_slice)
    }
}

struct Tokens<'a>(&'a str);

impl<'a> Iterator for Tokens<'a> {
    type Item = (&'a str, bool);

    fn next(&mut self) -> Option<Self::Item> {
        let text = self.0.trim_start_matches(xml_space);
        if text.is_empty() {
            return None;
        }
        if let Some((string, remaining)) = quoted(text) {
            self.0 = remaining;
            return Some((string, true));
        }
        let end = if text.starts_with('(') {
            text.find(')').map(|index| index + 1)?
        } else {
            text.find(xml_space).unwrap_or(text.len())
        };
        let (token, remaining) = text.split_at(end);
        self.0 = remaining;
        Some((token, false))
    }
}
