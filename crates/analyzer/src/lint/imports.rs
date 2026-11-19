//! In `use App\Client as C; new C();`, the name resolves through an import.
//! Class, function and constant aliases have independent PHP name tables.

use super::{
    Context,
    fix::{Edit, Fix},
    rule::{DefaultStatus, Group, Rule, Visit},
    source::is_comment,
};
use phera_ast::{Name, Statement, StatementKind, UseItem, UseKind};
use phera_lexer::TokenKind;
use phera_source::Span;
use std::collections::HashSet;

pub(super) const RULES: &[Rule] = &[Rule {
    name: "unused-import",
    group: Group::Unused,
    description: "Remove namespace imports that have no references.",
    default: DefaultStatus::Enabled,
    bad: "<?php\nuse App\\Unused;\necho 1;",
    good: "<?php\necho 1;",
    fix: "Safe fix. Comments inside an import prevent removing the affected text.",
    visit: Visit::Namespace,
    check: None,
}];

type Key = (UseKind, String);

pub(super) struct NamespaceImports {
    active: bool,
    references: HashSet<Key>,
}

impl NamespaceImports {
    pub(super) fn new(statements: &[Statement], enabled: bool) -> Self {
        Self {
            active: enabled
                && statements
                    .iter()
                    .any(|s| matches!(s.kind, StatementKind::Use(_))),
            references: HashSet::new(),
        }
    }

    pub(super) fn reference(&mut self, name: &Name, kind: UseKind) {
        if self.active
            && let Some(key) = crate::names::import_key(name, kind)
        {
            self.references.insert(key);
        }
    }

    pub(super) fn report(&self, context: &mut Context<'_>, statements: &[Statement]) {
        if !self.active {
            return;
        }
        let docs = doc_words(context);
        for statement in statements {
            let StatementKind::Use(items) = &statement.kind else {
                continue;
            };
            for (position, item) in items.iter().enumerate() {
                let alias = crate::names::alias(item);
                let key = (item.kind, crate::names::import_case(item.kind, alias));
                if self.references.contains(&key) || docs.contains(&alias.to_ascii_lowercase()) {
                    continue;
                }
                let edit = removal(context, statement, items, position);
                context.record(
                    "unused-import",
                    context.lines.line(item.span.start),
                    if item.alias.is_some() {
                        format!("Import '{}' as '{alias}' is unused.", item.name.text)
                    } else {
                        format!("Import '{}' is unused.", item.name.text)
                    },
                    edit.map(|edit| Fix::safe(vec![edit])),
                );
            }
        }
    }
}

fn doc_words(context: &Context<'_>) -> HashSet<String> {
    if !context.source.bytes.windows(3).any(|part| part == b"/**") {
        return HashSet::new();
    }
    context
        .source
        .tokens()
        .iter()
        .filter(|token| token.kind == TokenKind::DocComment)
        .flat_map(|token| {
            context
                .source
                .slice(token.span)
                .split(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_' || !byte.is_ascii()))
        })
        .filter_map(|word| std::str::from_utf8(word).ok())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn removal(
    context: &Context<'_>,
    statement: &Statement,
    items: &[UseItem],
    position: usize,
) -> Option<Edit> {
    let item = items.get(position)?;
    let span = if items.len() == 1 {
        whole_import(context, statement.span)?
    } else if let Some(next) = items.get(position + 1) {
        // Leave `use`, the group brace and the import kind in place.
        let start = item.span.start.max(item.name.span.start);
        let start = if position == 0 && item.span.start == statement.span.start {
            start
        } else {
            item.span.start
        };
        Span::new(start, next.span.start)
    } else {
        let previous = items.get(position.checked_sub(1)?)?;
        let between = Span::new(previous.span.end, item.span.start);
        let comma = context
            .source
            .within(between)
            .iter()
            .find(|token| token.kind == TokenKind::Comma)?;
        Span::new(comma.span.start, item.span.end)
    };
    let tokens = context.source.within(span);
    if tokens.iter().any(is_comment) {
        return None;
    }
    // A multi-line import can affect __LINE__ and source locations. Retain its
    // existing line endings when removing PHP tokens.
    let replacement: Vec<u8> = context
        .source
        .slice(span)
        .iter()
        .copied()
        .filter(|byte| matches!(byte, b'\r' | b'\n'))
        .collect();
    if replacement.is_empty() {
        Some(Edit::remove(span))
    } else {
        Some(Edit { span, replacement })
    }
}

fn whole_import(context: &Context<'_>, span: Span) -> Option<Span> {
    let tokens = context.source.within(span);
    let start = tokens
        .first()
        .filter(|token| token.kind == TokenKind::Use)?
        .span
        .start;
    let end = tokens
        .last()
        .filter(|token| token.kind == TokenKind::Semicolon)?
        .span
        .end;
    Some(Span::new(start, end))
}
