//! PHP name resolution: `namespace App; use Vendor\Thing as T; new T;`.
//! Class aliases are case insensitive; constant aliases retain their case.

use std::collections::BTreeMap;

use phera_ast::{Name, NameKind, Statement, StatementKind, UseKind};

#[derive(Clone, Debug, Default)]
pub struct Names {
    pub namespace: String,
    classes: BTreeMap<String, String>,
    functions: BTreeMap<String, String>,
    constants: BTreeMap<String, String>,
}

impl Names {
    pub fn for_block(namespace: String, statements: &[Statement]) -> Self {
        let mut names = Self {
            namespace,
            ..Self::default()
        };
        for statement in statements {
            if let StatementKind::Use(items) = &statement.kind {
                for item in items {
                    let alias = item
                        .alias
                        .as_ref()
                        .map(|a| a.name.as_str())
                        .unwrap_or_else(|| item.name.text.rsplit('\\').next().unwrap_or(""));
                    let (map, key) = match item.kind {
                        UseKind::Class => (&mut names.classes, alias.to_ascii_lowercase()),
                        UseKind::Function => (&mut names.functions, alias.to_ascii_lowercase()),
                        UseKind::Constant => (&mut names.constants, alias.to_string()),
                    };
                    map.insert(key, item.name.text.clone());
                }
            }
        }
        names
    }

    pub fn declaration(&self, name: &str) -> String {
        if self.namespace.is_empty() {
            name.into()
        } else {
            format!("{}\\{name}", self.namespace)
        }
    }

    pub fn resolve(&self, name: &Name, kind: UseKind) -> Vec<String> {
        match name.kind {
            NameKind::FullyQualified => return vec![name.text.clone()],
            NameKind::Relative => return vec![self.declaration(&name.text)],
            _ => {}
        }
        let (head, tail) = name.text.split_once('\\').unwrap_or((&name.text, ""));
        let map = match kind {
            UseKind::Class => &self.classes,
            UseKind::Function if tail.is_empty() => &self.functions,
            UseKind::Constant if tail.is_empty() => &self.constants,
            _ => &self.classes,
        };
        let key = if kind == UseKind::Constant && tail.is_empty() {
            head.to_string()
        } else {
            head.to_ascii_lowercase()
        };
        if let Some(imported) = map.get(&key) {
            return vec![if tail.is_empty() {
                imported.clone()
            } else {
                format!("{imported}\\{tail}")
            }];
        }
        let mut candidates = vec![self.declaration(&name.text)];
        if kind != UseKind::Class
            && name.kind == NameKind::Unqualified
            && !self.namespace.is_empty()
        {
            candidates.push(name.text.clone());
        }
        candidates
    }
}
