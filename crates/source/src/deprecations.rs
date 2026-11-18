//! The PHP stub deprecations and Phera's compatibility policy.
//!
//! ```php
//! strftime('%Y');  // Phera\NotSupportedException
//! echo E_STRICT;   // PHP's deprecation notice, then the value
//! ```

use std::{collections::HashMap, sync::OnceLock};

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum ItemKind {
    Function,
    Constant,
    Method,
    ClassConstant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Keep,
    Omit,
}

pub struct Deprecation {
    pub name: &'static str,
    pub since: &'static str,
    pub message: &'static str,
    pub policy: Policy,
    kind: ItemKind,
    constant_notice: OnceLock<String>,
}

impl Deprecation {
    pub fn unsupported_message(&self) -> String {
        let parentheses = match self.kind {
            ItemKind::Function | ItemKind::Method => "()",
            ItemKind::Constant | ItemKind::ClassConstant => "",
        };
        let mut message = format!("{}{parentheses} is not supported by Phera: ", self.name);
        if self.since.is_empty() {
            message.push_str("deprecated by phpredis");
        } else {
            message.push_str("deprecated since PHP ");
            message.push_str(self.since);
        }
        append_message(&mut message, self.message);
        message
    }

    fn constant_notice(&self) -> &str {
        self.constant_notice.get_or_init(|| {
            let mut message = format!("Constant {} is deprecated since {}", self.name, self.since);
            append_message(&mut message, self.message);
            message
        })
    }
}

fn append_message(message: &mut String, suffix: &str) {
    if !suffix.is_empty() {
        message.push_str(", ");
        message.push_str(suffix);
    }
}

const DATA: &str = include_str!("../data/deprecations.tsv");
type Deprecations = HashMap<(ItemKind, String), Deprecation>;
static ITEMS: OnceLock<Deprecations> = OnceLock::new();

fn key(kind: ItemKind, name: &str) -> String {
    match kind {
        ItemKind::Function | ItemKind::Method => name.to_ascii_lowercase(),
        ItemKind::Constant => name.to_owned(),
        ItemKind::ClassConstant => match name.split_once("::") {
            Some((class, constant)) => format!("{}::{constant}", class.to_ascii_lowercase()),
            None => name.to_owned(),
        },
    }
}

pub fn find(kind: ItemKind, name: &str) -> Option<&'static Deprecation> {
    ITEMS
        .get_or_init(|| {
            DATA.lines()
                .filter_map(|line| {
                    let mut fields = line.split('\t');
                    let kind = match fields.next()? {
                        "function" => ItemKind::Function,
                        "constant" => ItemKind::Constant,
                        "method" => ItemKind::Method,
                        "class-constant" => ItemKind::ClassConstant,
                        _ => return None,
                    };
                    let name = fields.next()?;
                    let since = fields.next()?;
                    let message = fields.next()?;
                    let policy = match fields.next()? {
                        "keep" => Policy::Keep,
                        "omit" => Policy::Omit,
                        _ => return None,
                    };
                    Some((
                        (kind, key(kind, name)),
                        Deprecation {
                            name,
                            since,
                            message,
                            policy,
                            kind,
                            constant_notice: OnceLock::new(),
                        },
                    ))
                })
                .collect()
        })
        .get(&(kind, key(kind, name.trim_start_matches('\\'))))
}

pub fn omitted(kind: ItemKind, name: &str) -> Option<&'static Deprecation> {
    find(kind, name).filter(|item| item.policy == Policy::Omit)
}

pub fn kept_constant_notice(kind: ItemKind, name: &str) -> Option<&'static str> {
    find(kind, name)
        .filter(|item| item.policy == Policy::Keep)
        .map(Deprecation::constant_notice)
}
