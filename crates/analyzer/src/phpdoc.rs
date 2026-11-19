//! Named PHPDoc declarations: `/** @method string render() */` supplies a method.
//! Only tags needed for symbol lookup and flow are read at levels 0 and 1.

use crate::names::Names;
use phera_ast::{Name, NameKind, UseKind};
use phera_source::Span;

#[derive(Clone, Debug, Default)]
pub struct Members {
    pub methods: std::collections::BTreeSet<String>,
    pub static_methods: std::collections::BTreeSet<String>,
    pub properties: std::collections::BTreeSet<String>,
    pub mixins: Vec<String>,
}

pub fn text(source: &str, span: Option<Span>) -> Option<&str> {
    let span = span?;
    source
        .get(span.start..span.end)?
        .strip_prefix("/**")?
        .strip_suffix("*/")
}

fn tags(doc: &str) -> impl Iterator<Item = (&str, &str)> {
    doc.lines().filter_map(|line| {
        let line = line.trim().trim_start_matches('*').trim_start();
        let (tag, rest) = line.split_once(char::is_whitespace)?;
        Some((tag, rest.trim_start()))
    })
}

pub fn members(doc: &str, names: &Names) -> Members {
    let mut members = Members::default();
    for (tag, rest) in tags(doc) {
        match tag {
            "@method" => {
                if let Some((head, _)) = rest.split_once('(')
                    && let Some(name) = head.split_whitespace().last()
                {
                    let methods =
                        if head.starts_with("static ") && head.split_whitespace().count() > 2 {
                            &mut members.static_methods
                        } else {
                            &mut members.methods
                        };
                    methods.insert(name.to_ascii_lowercase());
                }
            }
            "@property" | "@property-read" | "@property-write" => {
                if let Some(name) = rest
                    .split_whitespace()
                    .find_map(|text| text.strip_prefix('$'))
                {
                    members.properties.insert(name.into());
                }
            }
            "@mixin" => {
                if let Some(name) = rest
                    .split_whitespace()
                    .next()
                    .and_then(|ty| class_name(ty, names))
                {
                    members.mixins.push(name);
                }
            }
            _ => {}
        }
    }
    members
}

pub fn never_returns(doc: &str) -> bool {
    tags(doc).any(|(tag, rest)| {
        matches!(tag, "@return" | "@phpstan-return")
            && matches!(
                rest.split_whitespace().next(),
                Some("never" | "never-return" | "no-return")
            )
    })
}

pub fn closure_receiver(doc: &str, parameter: &str, names: &Names) -> Option<String> {
    tags(doc).find_map(|(tag, rest)| {
        if tag != "@param-closure-this" {
            return None;
        }
        let mut words = rest.split_whitespace();
        let ty = words.next()?;
        (words.next()?.strip_prefix('$')? == parameter)
            .then(|| class_name(ty, names))
            .flatten()
    })
}

fn class_name(ty: &str, names: &Names) -> Option<String> {
    let ty = ty.split('<').next()?;
    let (kind, text) = if let Some(absolute) = ty.strip_prefix('\\') {
        (NameKind::FullyQualified, absolute)
    } else if ty.contains('\\') {
        (NameKind::Qualified, ty)
    } else {
        (NameKind::Unqualified, ty)
    };
    if !text
        .chars()
        .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '\\')
        || text.is_empty()
    {
        return None;
    }
    if matches!(text, "self" | "static" | "parent") {
        return Some(text.into());
    }
    names
        .resolve(
            &Name {
                text: text.into(),
                kind,
                span: Span::new(0, 0),
            },
            UseKind::Class,
        )
        .into_iter()
        .next()
}
