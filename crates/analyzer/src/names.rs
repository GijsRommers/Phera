//! PHP name resolution: `namespace App; use Vendor\Thing as T; new T;`.
//! Class aliases are case insensitive; constant aliases retain their case.

use std::collections::BTreeMap;

use phera_ast::{Name, NameKind, Statement, StatementKind, UseItem, UseKind};

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
                    let map = match item.kind {
                        UseKind::Class => &mut names.classes,
                        UseKind::Function => &mut names.functions,
                        UseKind::Constant => &mut names.constants,
                    };
                    let key = import_case(item.kind, alias(item));
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
        let Some((import_kind, key)) = import_key(name, kind) else {
            return vec![match name.kind {
                NameKind::Relative => self.declaration(&name.text),
                _ => name.text.clone(),
            }];
        };
        let tail = name.text.split_once('\\').map_or("", |(_, tail)| tail);
        let map = match import_kind {
            UseKind::Class => &self.classes,
            UseKind::Function => &self.functions,
            UseKind::Constant => &self.constants,
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

pub(crate) fn alias(item: &UseItem) -> &str {
    item.alias
        .as_ref()
        .map(|alias| alias.name.as_str())
        .unwrap_or_else(|| item.name.text.rsplit('\\').next().unwrap_or_default())
}

pub(crate) fn import_case(kind: UseKind, name: &str) -> String {
    match kind {
        UseKind::Constant => name.into(),
        UseKind::Class | UseKind::Function => name.to_ascii_lowercase(),
    }
}

pub(crate) fn import_key(name: &Name, kind: UseKind) -> Option<(UseKind, String)> {
    match name.kind {
        NameKind::FullyQualified | NameKind::Relative => None,
        NameKind::Qualified => {
            let head = name
                .text
                .split_once('\\')
                .map_or(name.text.as_str(), |(head, _)| head);
            Some((UseKind::Class, import_case(UseKind::Class, head)))
        }
        NameKind::Unqualified => Some((kind, import_case(kind, &name.text))),
    }
}
