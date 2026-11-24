//! Resolve a project reference such as `new Vendor\\Client` to declarations.
//! A dependency's body never enters the analyzer's reporting queue.

use crate::{collect, composer::Autoload, index::Index, project, traits};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Dependencies {
    autoload: Option<Autoload>,
    seen_files: BTreeSet<PathBuf>,
    unlocated_classes: BTreeSet<String>,
    resolved_classes: BTreeSet<String>,
    pub vendor_files: usize,
}

impl Dependencies {
    pub fn load(index: &mut Index, paths: &[String], checked: &[PathBuf]) -> Result<Self, String> {
        let autoload = Autoload::discover(paths)?;
        let mut dependencies = Self {
            autoload,
            seen_files: checked
                .iter()
                .filter_map(|p| p.canonicalize().ok())
                .collect(),
            ..Self::default()
        };
        if let Some(autoload) = &dependencies.autoload {
            dependencies.seen_files = checked
                .iter()
                .filter_map(|p| p.canonicalize().ok())
                .collect();
            for path in autoload.files.clone() {
                dependencies.load_file(index, &path)?;
            }
        }
        Ok(dependencies)
    }

    pub fn load_file(&mut self, index: &mut Index, path: &Path) -> Result<(), String> {
        let canonical = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if !self.seen_files.insert(canonical.clone()) {
            return Ok(());
        }
        let parsed = project::parse_file(path)?;
        if self
            .autoload
            .as_ref()
            .is_some_and(|a| path.starts_with(&a.vendor) || canonical.starts_with(&a.vendor))
        {
            self.vendor_files += 1;
        }
        if let Ok(file) = parsed {
            let mut declarations = Index {
                read_type_docs: index.read_type_docs,
                ..Index::default()
            };
            collect::collect(&mut declarations, &file.path, &file.source, &file.program);
            // A dependency may contain a polyfill or declarations already found
            // in the requested paths. Keep those callers' original signatures.
            for (name, class) in declarations.classes {
                if let std::collections::btree_map::Entry::Vacant(entry) = index.classes.entry(name)
                {
                    index.trait_users.record(&class);
                    entry.insert(class);
                }
            }
            for (name, function) in declarations.functions {
                index.functions.entry(name).or_insert(function);
            }
            for (name, constant) in declarations.constants {
                index.constants.entry(name).or_insert(constant);
            }
        }
        Ok(())
    }

    pub fn resolve(&mut self, index: &mut Index, class: &str) {
        if self.autoload.is_some() {
            self.resolve_classes(index, vec![class.into()]);
        }
    }

    pub fn resolve_classes(&mut self, index: &mut Index, mut pending: Vec<String>) {
        let Some(_) = &self.autoload else {
            crate::phpdoc::type_aliases::resolve(index, pending);
            return;
        };
        let mut adaptations = Vec::new();
        let mut alias_classes = Vec::new();
        while let Some(name) = pending.pop() {
            let key = name.to_ascii_lowercase();
            if self.resolved_classes.contains(&key) {
                continue;
            }
            if !index.classes.contains_key(&key)
                && self.unlocated_classes.insert(key.clone())
                && let Some(path) = self
                    .autoload
                    .as_ref()
                    .and_then(|a| a.class_file(&name, &key))
            {
                // A stale classmap entry behaves like an unresolved class. Any
                // diagnostic belongs to the code referencing it.
                let _ = self.load_file(index, &path);
            }
            if let Some(class) = index.classes.get(&key) {
                self.resolved_classes.insert(key.clone());
                pending.extend(
                    class
                        .parents
                        .iter()
                        .chain(&class.interfaces)
                        .chain(&class.traits)
                        .chain(&class.documented.mixins)
                        .chain(
                            class
                                .documented
                                .type_imports
                                .iter()
                                .map(|import| &import.source),
                        )
                        .cloned(),
                );
                if !class.documented.type_imports.is_empty() {
                    alias_classes.push(key.clone());
                }
                if !class.adaptations.is_empty() {
                    adaptations.push(key);
                }
            }
        }
        crate::phpdoc::type_aliases::resolve(index, alias_classes);
        traits::link_classes(index, &adaptations);
    }
}
