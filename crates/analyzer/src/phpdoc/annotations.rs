//! Attach `@param User $user` to its parameter and keep the tag's diagnostic span.
//! PHPStan-prefixed tags take precedence over Psalm-prefixed or unprefixed tags.

use super::type_parser::{Templates, parse};
use crate::{names::Names, types::ValueType};
use phera_source::Span;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Annotation {
    pub ty: ValueType,
    pub span: Span,
    priority: Priority,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Documentation {
    pub params: BTreeMap<String, Annotation>,
    pub assertions: Vec<super::assertions::Assertion>,
    pub returns: Option<Annotation>,
    pub variables: BTreeMap<Option<String>, Annotation>,
    pub templates: Templates,
    pub template_names: Vec<String>,
    pub bases: Vec<Annotation>,
    pub properties: BTreeMap<String, Annotation>,
    pub methods: BTreeMap<String, Annotation>,
    pub errors: Vec<Error>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Priority {
    Standard,
    Psalm,
    PhpStan,
}

struct Tag<'a> {
    name: &'a str,
    body: String,
    offset: usize,
}

pub fn read(
    source: &str,
    span: Option<Span>,
    names: &Names,
    inherited: &Templates,
) -> Documentation {
    let mut documentation = Documentation::default();
    let Some(span) = span else {
        return documentation;
    };
    let Some(text) = super::text(source, Some(span)) else {
        return documentation;
    };
    let tags = detailed_tags(text, span.start + 3);
    let mut templates = inherited.clone();
    for tag in &tags {
        let (_, name) = tag_name(tag.name);
        if (template_tag(name) || matches!(name, "type" | "import-type"))
            && let Some(name) = tag.body.split_whitespace().next()
        {
            templates.insert(name.into(), ValueType::Mixed);
        }
    }
    for tag in &tags {
        let (_, name) = tag_name(tag.name);
        if template_tag(name)
            && let Some(name) = tag.body.split_whitespace().next()
        {
            let rest = tag.body[name.len()..].trim_start();
            let bound = rest
                .split_once(char::is_whitespace)
                .and_then(|(operator, bound)| {
                    if matches!(operator, "of" | "as") {
                        parse(bound.trim_start(), names, &templates)
                            .ok()
                            .map(|parsed| parsed.ty)
                    } else {
                        None
                    }
                })
                .unwrap_or(ValueType::Mixed);
            if !documentation.template_names.iter().any(|old| old == name) {
                documentation.template_names.push(name.into());
            }
            templates.insert(name.into(), bound.limit_complexity());
        }
    }
    for tag in &tags {
        let (_, name) = tag_name(tag.name);
        if name == "type"
            && let Some((alias, body)) = tag.body.split_once(char::is_whitespace)
        {
            let body = body.trim_start().trim_start_matches('=').trim_start();
            if let Ok(parsed) = parse(body, names, &templates) {
                templates.insert(alias.into(), parsed.ty.limit_complexity());
            }
        }
        if name == "import-type" {
            let mut words = tag.body.split_whitespace();
            if let Some(local) = words.next() {
                let local = words
                    .skip_while(|word| *word != "as")
                    .nth(1)
                    .unwrap_or(local);
                templates.insert(local.into(), ValueType::Mixed);
            }
        }
    }
    documentation.templates = templates;
    for tag in &tags {
        let (priority, name) = tag_name(tag.name);
        let span = Span::new(tag.offset, tag.offset);
        if name == "assert" && priority != Priority::Standard {
            if let Some(assertion) =
                super::assertions::parse_assertion(&tag.body, names, &documentation.templates)
            {
                documentation.assertions.push(assertion);
            }
            continue;
        }
        if name == "method" {
            match method(&tag.body, names, &documentation.templates) {
                Some((name, ty)) => insert(
                    &mut documentation.methods,
                    name,
                    Annotation { ty, span, priority },
                ),
                None => documentation.errors.push(Error {
                    span,
                    message: format!("PHPDoc tag {} has an invalid method signature.", tag.name),
                }),
            }
            continue;
        }
        if !matches!(
            name,
            "param"
                | "param-out"
                | "return"
                | "var"
                | "extends"
                | "implements"
                | "use"
                | "property"
                | "property-read"
                | "property-write"
                | "mixin"
        ) {
            continue;
        }
        if matches!(name, "param" | "param-out")
            && let Some(variable) = variable_suffix(&tag.body)
        {
            if name == "param" {
                insert(
                    &mut documentation.params,
                    variable.into(),
                    Annotation {
                        ty: ValueType::Mixed,
                        span,
                        priority,
                    },
                );
            }
            continue;
        }
        let parsed = match parse(&tag.body, names, &documentation.templates) {
            Ok(parsed) => parsed,
            Err(error) => {
                documentation.errors.push(Error {
                    span,
                    message: format!(
                        "PHPDoc tag {} has invalid value ({}): expected {} at offset {}.",
                        tag.name, tag.body, error.expected, error.offset
                    ),
                });
                continue;
            }
        };
        let annotation = Annotation {
            ty: parsed.ty,
            span,
            priority,
        };
        match name {
            "param" | "param-out" => {
                if let Some(variable) = variable_suffix(parsed.suffix) {
                    if name == "param" {
                        insert(&mut documentation.params, variable.to_string(), annotation);
                    }
                } else {
                    documentation.errors.push(Error {
                        span,
                        message: format!("PHPDoc tag {} expects a parameter name.", tag.name),
                    });
                }
            }
            "var" => insert(
                &mut documentation.variables,
                variable_suffix(parsed.suffix).map(str::to_string),
                annotation,
            ),
            "return" => {
                if documentation
                    .returns
                    .as_ref()
                    .is_none_or(|old| priority >= old.priority)
                {
                    documentation.returns = Some(annotation);
                }
            }
            "property" | "property-read" | "property-write" => {
                if let Some(variable) = variable_suffix(parsed.suffix) {
                    insert(&mut documentation.properties, variable.into(), annotation);
                }
            }
            _ => documentation.bases.push(annotation),
        }
    }
    documentation
}

fn method(body: &str, names: &Names, templates: &Templates) -> Option<(String, ValueType)> {
    let body = body.trim();
    let (static_method, body) = match body.strip_prefix("static ") {
        Some(rest)
            if rest
                .split_once('(')
                .is_some_and(|(head, _)| head.split_whitespace().count() >= 2) =>
        {
            (true, rest)
        }
        _ => (false, body),
    };
    let parsed = parse(body, names, templates).ok()?;
    let (name, parameters, returns) = if parsed.suffix.starts_with('(') {
        let name = body.split_once('(')?.0.trim();
        (name, parsed.suffix, ValueType::Mixed)
    } else {
        let (name, parameters) = parsed.suffix.split_once('(')?;
        (name.trim(), parameters, parsed.ty)
    };
    if name.is_empty() || !name.chars().all(|ch| ch.is_alphanumeric() || ch == '_') {
        return None;
    }
    let parameters = parameters.trim_start_matches('(');
    let function = format!("callable({parameters}");
    let ValueType::Callable(signature) = parse(&function, names, templates).ok()?.ty else {
        return None;
    };
    let mut signature = (*signature).clone();
    signature.returns = returns;
    signature.modifiers.is_static = static_method;
    Some((
        name.to_ascii_lowercase(),
        ValueType::Callable(std::sync::Arc::new(signature)),
    ))
}

fn variable_suffix(suffix: &str) -> Option<&str> {
    let name = suffix
        .trim_start()
        .trim_start_matches(['&', '.'])
        .trim_start()
        .strip_prefix('$')?;
    let end = name
        .char_indices()
        .find(|(_, ch)| !(ch.is_alphanumeric() || *ch == '_'))
        .map_or(name.len(), |(i, _)| i);
    (end > 0).then(|| &name[..end])
}

fn insert<K: Ord>(out: &mut BTreeMap<K, Annotation>, key: K, value: Annotation) {
    if out
        .get(&key)
        .is_none_or(|old| value.priority >= old.priority)
    {
        out.insert(key, value);
    }
}

fn template_tag(tag: &str) -> bool {
    matches!(
        tag,
        "template" | "template-covariant" | "template-contravariant"
    )
}

fn tag_name(tag: &str) -> (Priority, &str) {
    if let Some(name) = tag.strip_prefix("@phpstan-") {
        (Priority::PhpStan, name)
    } else if let Some(name) = tag.strip_prefix("@psalm-") {
        (Priority::Psalm, name)
    } else {
        (Priority::Standard, tag.trim_start_matches('@'))
    }
}

fn detailed_tags(text: &str, start: usize) -> Vec<Tag<'_>> {
    let mut result: Vec<Tag<'_>> = Vec::new();
    let mut offset = start;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim().trim_start_matches('*').trim_start();
        if trimmed.starts_with('@') {
            let (name, value) = trimmed
                .split_once(char::is_whitespace)
                .unwrap_or((trimmed, ""));
            result.push(Tag {
                name,
                body: value.trim().into(),
                offset: offset + line.find('@').unwrap_or(0),
            });
        } else if let Some(tag) = result.last_mut()
            && !trimmed.is_empty()
        {
            tag.body.push('\n');
            tag.body.push_str(trimmed);
        }
        offset += line.len();
    }
    result
}
