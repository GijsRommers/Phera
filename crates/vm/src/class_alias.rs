//! `class_alias(Original::class, 'Other')` adds a second name for the same class identity.

use std::collections::hash_map::Entry;

use phera_extension_api::NativeResult;
use phera_value::Value;

use crate::Vm;

impl Vm {
    /// An alias never duplicates code, metadata, or static property storage.
    pub(crate) async fn class_alias(
        &mut self,
        name: &str,
        alias: &str,
        autoload: bool,
    ) -> NativeResult {
        let class = if autoload {
            self.lookup_class(name).await?
        } else {
            self.classes.get(name)
        };
        let Some(class) = class else {
            self.warn(format!("Class \"{name}\" not found"));
            return Ok(Value::Bool(false));
        };
        let key = alias
            .strip_prefix('\\')
            .unwrap_or(alias)
            .to_ascii_lowercase();
        match self.classes.entries.entry(key.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(class);
                self.classes.order.push(key);
                Ok(Value::Bool(true))
            }
            Entry::Occupied(_) => {
                let kind = match class.kind {
                    phera_bytecode::ClassKind::Interface => "interface",
                    _ => "class",
                };
                let mut message = format!("Cannot redeclare {kind} {alias}");
                if let Some((function, line)) = class.declaration {
                    let file = &self.code.unit(function).file;
                    message.push_str(&format!(" (previously declared in {file}:{line})"));
                }
                self.warn(message);
                Ok(Value::Bool(false))
            }
        }
    }

    /// Membership uses canonical names on a miss so aliases work for ancestors as well.
    pub(crate) fn class_matches(&self, class: &crate::classes::Class, name: &str) -> bool {
        class.is_a(name) || self.alias_matches(class, name)
    }

    #[cold]
    pub(crate) fn alias_matches(&self, class: &crate::classes::Class, name: &str) -> bool {
        self.classes
            .get(name)
            .is_some_and(|target| class.is_a(&target.name))
    }
}
