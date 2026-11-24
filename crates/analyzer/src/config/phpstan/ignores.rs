//! `ignoreErrors:` can match diagnostics by PCRE message, identifier and path.
//! An entry with `count: 1` can suppress only its first matching finding.

use super::lines::{blocks, key_value, list, scalar};
use crate::diagnostic::Diagnostic;
use pcre2::bytes::{Regex, RegexBuilder};
use std::path::Path;

#[derive(Clone, Default)]
pub struct Ignore {
    messages: Vec<Regex>,
    raw_messages: Vec<String>,
    identifiers: Vec<String>,
    paths: Vec<String>,
    count: Option<usize>,
}

impl Ignore {
    pub(crate) fn filter(ignores: &[Self], findings: &mut Vec<Diagnostic>) {
        if ignores.is_empty() {
            return;
        }
        let mut remaining: Vec<_> = ignores
            .iter()
            .map(|ignore| ignore.count.unwrap_or(usize::MAX))
            .collect();
        let mut last_file = String::new();
        let mut absolute = None;
        findings.retain(|diagnostic| {
            if last_file != diagnostic.file {
                last_file.clone_from(&diagnostic.file);
                absolute = Path::new(&last_file)
                    .canonicalize()
                    .ok()
                    .map(|path| crate::project::display_path(&path));
            }
            for (ignore, remaining) in ignores.iter().zip(&mut remaining) {
                if *remaining > 0 && ignore.matches(diagnostic, absolute.as_deref()) {
                    *remaining -= 1;
                    return false;
                }
            }
            true
        });
    }

    fn matches(&self, diagnostic: &Diagnostic, absolute: Option<&str>) -> bool {
        let message = diagnostic
            .message
            .split_once(" The type was narrowed on line ")
            .map_or(diagnostic.message.as_str(), |(base, _)| base);
        ((self.messages.is_empty() && self.raw_messages.is_empty())
            || self.raw_messages.iter().any(|raw| raw == message)
            || self
                .messages
                .iter()
                .any(|pattern| pattern.is_match(message.as_bytes()).unwrap_or(false)))
            && (self.identifiers.is_empty() || self.identifiers.contains(&diagnostic.identifier))
            && (self.paths.is_empty()
                || self.paths.iter().any(|path| {
                    crate::project::glob(path, &diagnostic.file)
                        || absolute.is_some_and(|file| crate::project::glob(path, file))
                }))
    }

    fn field(
        &mut self,
        line: &str,
        children: &[&str],
        resolve: &impl Fn(&str) -> String,
    ) -> Result<(), &'static str> {
        let (key, value) = key_value(line).ok_or("expected a key and value")?;
        match key {
            "message" => self.messages.push(pattern(&required_scalar(value)?)?),
            "messages" => {
                for text in list(value, children) {
                    self.messages.push(pattern(&text)?);
                }
            }
            "rawMessage" => self.raw_messages.push(required_scalar(value)?),
            "rawMessages" => self.raw_messages.extend(list(value, children)),
            "identifier" => self.identifiers.push(required_scalar(value)?),
            "identifiers" => self.identifiers.extend(list(value, children)),
            "path" => self.paths.push(resolve(&required_scalar(value)?)),
            "paths" => self
                .paths
                .extend(list(value, children).iter().map(|path| resolve(path))),
            "count" => self.count = Some(value.parse().map_err(|_| "invalid count")?),
            // This changes reporting of invalid suppressions, not their target.
            "reportUnmatched" => {}
            _ => return Err("unsupported entry key"),
        }
        Ok(())
    }

    fn entry(
        text: &str,
        children: &[&str],
        resolve: &impl Fn(&str) -> String,
    ) -> Result<Self, &'static str> {
        let mut entry = Self::default();
        if text.starts_with(['\'', '"']) || text.starts_with(['~', '%', '/']) {
            entry.messages.push(pattern(&required_scalar(text)?)?);
        } else if !text.is_empty() {
            entry.field(text, &[], resolve)?;
        }
        for (line, children) in blocks(children) {
            entry.field(line, children, resolve)?;
        }
        if entry.messages.is_empty()
            && entry.raw_messages.is_empty()
            && entry.identifiers.is_empty()
        {
            return Err("missing message or identifier");
        }
        Ok(entry)
    }
}

pub(super) fn read(
    value: &str,
    children: &[&str],
    resolve: impl Fn(&str) -> String,
) -> Vec<Result<Ignore, &'static str>> {
    if !value.is_empty() {
        return list(value, children)
            .into_iter()
            .map(|value| {
                Ok(Ignore {
                    messages: vec![pattern(&value)?],
                    ..Ignore::default()
                })
            })
            .collect();
    }
    blocks(children)
        .map(|(line, children)| {
            let item = line.strip_prefix('-').ok_or("expected a list item")?.trim();
            Ignore::entry(item, children, &resolve)
        })
        .collect()
}

fn required_scalar(text: &str) -> Result<String, &'static str> {
    scalar(text).ok_or("invalid string")
}

fn pattern(text: &str) -> Result<Regex, &'static str> {
    let delimiter = text
        .chars()
        .next()
        .filter(|ch| ch.is_ascii_punctuation() && *ch != '\\')
        .ok_or("expected a delimited message pattern")?;
    let closing = match delimiter {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '<' => '>',
        other => other,
    };
    let (expression, modifiers) = text[1..]
        .rsplit_once(closing)
        .ok_or("unclosed message pattern")?;
    let mut builder = RegexBuilder::new();
    for modifier in modifiers.chars() {
        match modifier {
            'i' => builder.caseless(true),
            'm' => builder.multi_line(true),
            's' => builder.dotall(true),
            'x' => builder.extended(true),
            'u' => builder.utf(true),
            _ => return Err("unsupported message pattern modifier"),
        };
    }
    builder
        .build(expression)
        .map_err(|_| "invalid message pattern")
}
