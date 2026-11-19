//! Expand internal general entities with request-independent resource limits.
//!
//! ```php
//! $document->loadXML('<!DOCTYPE r [<!ENTITY x "hi">]><r>&x;</r>', LIBXML_NOENT);
//! ```

use std::collections::{HashMap, HashSet};

use crate::document::ParseError;
use crate::tree::{Kind, Name, Node};

use super::encoding::xml_char;

const EXPANSION_BYTES: usize = 8 * 1024 * 1024;
const EXPANSION_STEPS: usize = 100_000;
const EXPANSION_DEPTH: usize = 32;

#[derive(Default)]
pub(super) struct Entities {
    definitions: HashMap<String, Option<String>>,
    bytes: usize,
    steps: usize,
}

pub(super) struct AttributeValue {
    pub(super) text: String,
    references: Vec<(usize, usize, String)>,
}

impl AttributeValue {
    pub(super) fn parts(&self) -> Vec<Node> {
        let mut nodes = Vec::new();
        if self.references.is_empty() {
            return nodes;
        }
        let mut tail = 0;
        for &(start, end, ref name) in &self.references {
            if start > tail {
                nodes.push(Node::new(
                    Kind::Text,
                    Name::new("#text"),
                    &self.text[tail..start],
                ));
            }
            nodes.push(Node::new(
                Kind::EntityReference,
                Name::new(name),
                &self.text[start..end],
            ));
            tail = end;
        }
        if tail < self.text.len() {
            nodes.push(Node::new(
                Kind::Text,
                Name::new("#text"),
                &self.text[tail..],
            ));
        }
        nodes
    }
}

impl Entities {
    pub(super) fn declare(&mut self, declaration: &str) {
        let text = declaration.trim_start();
        let Some((name, data)) = text.split_once(char::is_whitespace) else {
            return;
        };
        let data = data.trim_start();
        if name != "%" {
            self.definitions
                .entry(name.into())
                .or_insert_with(|| quoted(data).map(|(value, _)| value.into()));
        }
    }

    pub(super) fn resolve(&mut self, name: &str) -> Result<(String, bool), ParseError> {
        if let Some(value) = special(name)? {
            return Ok((value, false));
        }
        let Some(definition) = self.definitions.get(name) else {
            return Err(undefined(name));
        };
        let Some(definition) = definition.as_deref() else {
            return Ok((String::new(), true));
        };

        let mut output = String::new();
        let mut active = HashSet::from([name]);
        let mut stack = vec![(name, definition)];
        while let Some((entity, rest)) = stack.last_mut() {
            self.steps += 1;
            if self.steps > EXPANSION_STEPS {
                return Err(expansion_limit());
            }
            let Some((text, reference)) = rest.split_once('&') else {
                append(&mut self.bytes, &mut output, rest)?;
                active.remove(*entity);
                stack.pop();
                continue;
            };
            append(&mut self.bytes, &mut output, text)?;
            let (reference, after) = reference
                .split_once(';')
                .ok_or_else(|| ParseError::fatal(23, "EntityRef: expecting ';'"))?;
            *rest = after;
            if let Some(value) = special(reference)? {
                append(&mut self.bytes, &mut output, &value)?;
            } else {
                match self.definitions.get(reference) {
                    Some(Some(value)) => {
                        if stack.len() >= EXPANSION_DEPTH || !active.insert(reference) {
                            return Err(expansion_limit());
                        }
                        stack.push((reference, value));
                    }
                    Some(None) => {}
                    None => return Err(undefined(reference)),
                }
            }
        }
        Ok((output, true))
    }

    pub(super) fn attribute(
        &mut self,
        mut input: &str,
        substitute: bool,
    ) -> Result<AttributeValue, ParseError> {
        let mut value = AttributeValue {
            text: String::new(),
            references: Vec::new(),
        };
        if !input.contains(['&', '\r', '\n', '\t']) {
            value.text = input.to_owned();
            return Ok(value);
        }
        while let Some((text, rest)) = input.split_once('&') {
            value.text.extend(text.chars().map(attribute_space));
            let (name, after) = rest
                .split_once(';')
                .ok_or_else(|| ParseError::fatal(23, "EntityRef: expecting ';'"))?;
            let (replacement, is_entity) = self.resolve(name)?;
            if is_entity {
                let start = value.text.len();
                value.text.extend(replacement.chars().map(attribute_space));
                if !substitute {
                    value
                        .references
                        .push((start, value.text.len(), name.to_owned()));
                }
            } else {
                value.text.push_str(&replacement);
            }
            input = after;
        }
        value.text.extend(input.chars().map(attribute_space));
        Ok(value)
    }
}

fn attribute_space(character: char) -> char {
    match character {
        '\r' | '\n' | '\t' => ' ',
        character => character,
    }
}

pub(super) fn quoted(input: &str) -> Option<(&str, &str)> {
    let quote = input.chars().next()?;
    if matches!(quote, '\'' | '"') {
        input.get(1..)?.split_once(quote)
    } else {
        None
    }
}

fn special(name: &str) -> Result<Option<String>, ParseError> {
    let character = match name {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        "apos" => '\'',
        "quot" => '"',
        reference if reference.starts_with('#') => {
            let number = match reference.strip_prefix("#x") {
                Some(hex) => u32::from_str_radix(hex, 16),
                None => reference[1..].parse(),
            };
            number
                .ok()
                .and_then(char::from_u32)
                .filter(|&c| xml_char(c))
                .ok_or_else(|| ParseError::fatal(9, "xmlParseCharRef: invalid xmlChar value"))?
        }
        _ => return Ok(None),
    };
    Ok(Some(character.to_string()))
}

fn undefined(name: &str) -> ParseError {
    ParseError::fatal(26, format!("Entity '{name}' not defined"))
}

fn expansion_limit() -> ParseError {
    ParseError::fatal(89, "Detected an entity reference loop")
}

fn append(total: &mut usize, output: &mut String, text: &str) -> Result<(), ParseError> {
    *total += text.len();
    if *total > EXPANSION_BYTES {
        return Err(expansion_limit());
    }
    output.push_str(text);
    Ok(())
}
