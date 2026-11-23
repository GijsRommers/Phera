//! Track ID and default attributes declared in the internal DTD subset.
//!
//! ```php
//! $document->loadXML('<!DOCTYPE r [<!ATTLIST r key ID #IMPLIED>]><r key="a"/>');
//! echo $document->getElementById('a')->nodeName;
//! ```

use std::collections::HashMap;
use std::fmt::Write;

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
    pub(super) fn declare(&mut self, data: &str, serialized: &mut String) {
        let mut tokens = Tokens(data);
        let Some((element, false)) = tokens.next() else {
            return;
        };
        let rules = self.0.entry(element.into()).or_default();
        while let Some((name, false)) = tokens.next() {
            let Some((kind, false)) = tokens.next() else {
                break;
            };
            let notation = if kind == "NOTATION" {
                tokens.next().map(|(group, _)| group)
            } else {
                None
            };
            let Some((value, quoted)) = tokens.next() else {
                break;
            };
            let default = match (value, quoted) {
                (_, true) => Some(value),
                ("#FIXED", false) => tokens
                    .next()
                    .filter(|&(_, quoted)| quoted)
                    .map(|(value, _)| value),
                _ => None,
            };
            if !rules.iter().any(|rule| rule.name == name) {
                let kind_text = format_model(kind);
                let _ = write!(serialized, "<!ATTLIST {element} {name} {kind_text}");
                if let Some(notation) = notation {
                    let _ = write!(serialized, " {}", format_model(notation));
                }
                if !quoted {
                    let _ = write!(serialized, " {value}");
                }
                if let Some(value) = default {
                    let value = value.replace('"', "&quot;");
                    let _ = write!(serialized, " \"{value}\"");
                }
                serialized.push_str(">\n");
                rules.push(AttributeRule {
                    name: name.into(),
                    is_id: kind == "ID",
                    default: default.map(str::to_owned),
                });
            }
        }
    }

    pub(super) fn get(&self, element: &str) -> &[AttributeRule] {
        self.0.get(element).map_or(&[], Vec::as_slice)
    }

    pub(super) fn defaults(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.0.iter().flat_map(|(element, rules)| {
            rules.iter().filter_map(move |rule| {
                Some((
                    element.as_str(),
                    rule.name.as_str(),
                    rule.default.as_deref()?,
                ))
            })
        })
    }
}

pub(super) fn write_element(data: &str, output: &mut String) -> bool {
    let Some((name, model)) = data.trim().split_once(xml_space) else {
        return false;
    };
    let model = format_model(model);
    if let Some(content) = model
        .strip_prefix('(')
        .and_then(|text| text.strip_suffix(')'))
        && !content.contains(['(', ')', ',', '|'])
        && content.ends_with(['+', '*', '?'])
    {
        let (child, quantifier) = content.split_at(content.len() - 1);
        let _ = writeln!(output, "<!ELEMENT {name} ({child}){quantifier}>");
    } else {
        let _ = writeln!(output, "<!ELEMENT {name} {model}>");
    }
    true
}

fn format_model(model: &str) -> String {
    let mut output = String::new();
    for character in model.chars().filter(|&character| !xml_space(character)) {
        match character {
            ',' => output.push_str(" , "),
            '|' => output.push_str(" | "),
            _ => output.push(character),
        }
    }
    output
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
