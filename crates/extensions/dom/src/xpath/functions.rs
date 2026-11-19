//! The XPath 1.0 core function library.
//!
//! ```php
//! $xpath->evaluate('normalize-space(string(//title))');
//! ```

use std::collections::{HashMap, HashSet};

use crate::tree::{Arena, Kind, NodeId};
use crate::xml::names::XML;

use super::value::{number, string_value};
use super::{Error, Eval, Value};

impl Eval<'_> {
    pub(super) fn function(&self, name: &str, arguments: &[Value]) -> Result<Value, Error> {
        use Value::{Boolean, Nodes, Number, String};

        let arena = self.arena;
        let context = self.node;
        let default_string = || string_value(arena, context);
        let str_arg = |arg: &Value| arg.string(arena);
        let num_arg = |arg: &Value| arg.number(arena);

        Ok(match (name, arguments) {
            ("last", []) => Number(self.size as f64),
            ("position", []) => Number(self.position as f64),
            ("count", [Nodes(nodes)]) => Number(nodes.len() as f64),
            ("count", [_]) => return Err(Error::Type),
            ("id", [value]) => Nodes(by_id(arena, value)),
            ("local-name" | "name" | "namespace-uri", []) => {
                String(node_name(arena, context, name))
            }
            ("local-name" | "name" | "namespace-uri", [Nodes(nodes)]) => String(
                nodes
                    .first()
                    .map_or_else(std::string::String::new, |&id| node_name(arena, id, name)),
            ),
            ("local-name" | "name" | "namespace-uri", [_]) => return Err(Error::Type),
            ("boolean", [value]) => Boolean(value.boolean()),
            ("not", [value]) => Boolean(!value.boolean()),
            ("true", []) => Boolean(true),
            ("false", []) => Boolean(false),
            ("lang", [language]) => Boolean(lang(arena, context, &str_arg(language))),
            ("string", []) => String(default_string()),
            ("string", [value]) => String(str_arg(value)),
            ("concat", [a, b, tail @ ..]) => {
                let mut string = str_arg(a);
                string.push_str(&str_arg(b));
                for value in tail {
                    string.push_str(&str_arg(value));
                }
                String(string)
            }
            ("starts-with", [a, b]) => Boolean(str_arg(a).starts_with(&str_arg(b))),
            ("contains", [a, b]) => Boolean(str_arg(a).contains(&str_arg(b))),
            ("substring-before", [a, b]) => String(
                str_arg(a)
                    .split_once(&str_arg(b))
                    .map_or_else(std::string::String::new, |(before, _)| before.to_owned()),
            ),
            ("substring-after", [a, b]) => String(
                str_arg(a)
                    .split_once(&str_arg(b))
                    .map_or_else(std::string::String::new, |(_, after)| after.to_owned()),
            ),
            ("substring", [a, b]) => String(substring(&str_arg(a), num_arg(b), None)),
            ("substring", [a, b, c]) => {
                String(substring(&str_arg(a), num_arg(b), Some(num_arg(c))))
            }
            ("string-length", []) => Number(default_string().chars().count() as f64),
            ("string-length", [value]) => Number(str_arg(value).chars().count() as f64),
            ("normalize-space", []) => String(normalize(&default_string())),
            ("normalize-space", [value]) => String(normalize(&str_arg(value))),
            ("translate", [a, b, c]) => String(translate(&str_arg(a), &str_arg(b), &str_arg(c))),
            ("number", []) => Number(number(&default_string())),
            ("number", [value]) => Number(num_arg(value)),
            ("sum", [Nodes(nodes)]) => Number(
                nodes
                    .iter()
                    .map(|&id| number(&string_value(arena, id)))
                    .fold(0.0, |sum, value| sum + value),
            ),
            ("sum", [_]) => return Err(Error::Type),
            ("floor", [value]) => Number(num_arg(value).floor()),
            ("ceiling", [value]) => Number(num_arg(value).ceil()),
            ("round", [value]) => Number(round(num_arg(value))),
            (name, _) if core_function(name) => return Err(Error::Arity),
            _ => return Err(Error::Function),
        })
    }
}

fn core_function(name: &str) -> bool {
    matches!(
        name,
        "last"
            | "position"
            | "count"
            | "id"
            | "local-name"
            | "name"
            | "namespace-uri"
            | "boolean"
            | "not"
            | "true"
            | "false"
            | "lang"
            | "string"
            | "concat"
            | "starts-with"
            | "contains"
            | "substring-before"
            | "substring-after"
            | "substring"
            | "string-length"
            | "normalize-space"
            | "translate"
            | "number"
            | "sum"
            | "floor"
            | "ceiling"
            | "round"
    )
}

fn node_name(arena: &Arena, id: NodeId, mode: &str) -> String {
    let node = &arena[id];
    if !matches!(
        node.kind,
        Kind::Element | Kind::Attribute | Kind::Namespace | Kind::ProcessingInstruction
    ) {
        return String::new();
    }
    match mode {
        "local-name" => node.name.local.clone(),
        "namespace-uri" => node.name.namespace.clone().unwrap_or_default(),
        _ => node.name.qualified(),
    }
}

fn by_id(arena: &Arena, value: &Value) -> Vec<NodeId> {
    let values = match value {
        Value::Nodes(nodes) => nodes.iter().map(|&id| string_value(arena, id)).collect(),
        value => vec![value.string(arena)],
    };
    let keys: HashSet<_> = values
        .iter()
        .flat_map(|value| value.split_ascii_whitespace())
        .collect();
    arena
        .content_descendants(Arena::DOCUMENT)
        .filter(|&id| {
            arena[id].kind == Kind::Element
                && arena[id]
                    .attributes()
                    .iter()
                    .any(|&attr| arena[attr].is_id && keys.contains(arena[attr].value.as_str()))
        })
        .collect()
}

fn lang(arena: &Arena, context: NodeId, language: &str) -> bool {
    let language = language.to_ascii_lowercase();
    std::iter::once(context)
        .chain(arena.ancestors(context))
        .find_map(|id| {
            arena[id].attributes().iter().find_map(|&attr| {
                let attr = &arena[attr];
                (attr.name.local == "lang" && attr.name.namespace.as_deref() == Some(XML))
                    .then(|| attr.value.to_ascii_lowercase())
            })
        })
        .is_some_and(|value| {
            value == language
                || value
                    .strip_prefix(&language)
                    .is_some_and(|rest| rest.starts_with('-'))
        })
}

fn normalize(string: &str) -> String {
    let mut output = String::with_capacity(string.len());
    for segment in string
        .split([' ', '\r', '\t', '\n'])
        .filter(|s| !s.is_empty())
    {
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(segment);
    }
    output
}

fn substring(value: &str, start: f64, length: Option<f64>) -> String {
    let start = round(start);
    let end = length.map_or(f64::INFINITY, |length| start + round(length));
    value
        .chars()
        .enumerate()
        .filter_map(|(index, character)| {
            let position = (index + 1) as f64;
            (position >= start && position < end).then_some(character)
        })
        .collect()
}

fn translate(value: &str, from: &str, to: &str) -> String {
    let mut translations = HashMap::new();
    let mut replacements = to.chars();
    for original in from.chars() {
        let replacement = replacements.next();
        translations.entry(original).or_insert(replacement);
    }
    value
        .chars()
        .filter_map(|character| {
            translations
                .get(&character)
                .copied()
                .unwrap_or(Some(character))
        })
        .collect()
}

fn round(number: f64) -> f64 {
    if (-0.5..0.0).contains(&number) {
        -0.0
    } else {
        let floor = number.floor();
        if number - floor >= 0.5 {
            floor + 1.0
        } else {
            floor
        }
    }
}
