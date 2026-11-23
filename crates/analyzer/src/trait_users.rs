//! Trait bodies get an importing class's context: `trait T { function f() { self::C; } }`.
//! Traits may use other traits, so the reverse traversal also follows trait users.

use crate::index::Index;
use phera_ast::ClassKind;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub struct TraitUsers(BTreeMap<String, BTreeSet<String>>);

impl TraitUsers {
    pub fn record(&mut self, class: &crate::index::ClassInfo) {
        for name in &class.traits {
            self.0
                .entry(name.to_ascii_lowercase())
                .or_default()
                .insert(class.name.clone());
        }
    }
}

impl Index {
    pub(crate) fn trait_has_users(&self, name: &str) -> bool {
        self.trait_users.0.contains_key(&name.to_ascii_lowercase())
    }

    pub fn trait_consumers(&self, name: &str) -> Vec<String> {
        let mut pending = vec![name];
        let mut seen = BTreeSet::new();
        let mut classes = BTreeSet::new();
        while let Some(name) = pending.pop() {
            if !seen.insert(name.to_ascii_lowercase()) {
                continue;
            }
            if let Some(users) = self.trait_users.0.get(&name.to_ascii_lowercase()) {
                for user in users {
                    if self
                        .class(user)
                        .is_some_and(|info| info.kind == ClassKind::Trait)
                    {
                        pending.push(user);
                    } else {
                        classes.insert(user.clone());
                    }
                }
            }
        }
        classes.into_iter().collect()
    }
}
