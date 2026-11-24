//! `@phpstan-import-type Row from Catalog as Entry` uses Catalog's type for Entry.
//! Bind imported aliases after collecting the source classes.

use crate::{
    index::{Index, Signature},
    names::Names,
    types::{RelativeTypes, Template, ValueType},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeImport {
    pub source: String,
    original: String,
    pub local: String,
}

impl TypeImport {
    pub(super) fn parse(body: &str, names: &Names) -> Option<Self> {
        let mut words = body.split_whitespace();
        let original = words.next()?;
        if words.next()? != "from" {
            return None;
        }
        let source = super::class_name(words.next()?, names)?;
        let local = if words.next() == Some("as") {
            words.next()?
        } else {
            original
        };
        Some(Self {
            source,
            original: original.into(),
            local: local.into(),
        })
    }
}

pub(super) fn placeholder(name: &str) -> ValueType {
    ValueType::Template(Arc::new(Template {
        name: name.into(),
        bound: ValueType::Mixed,
    }))
}

enum Visit {
    Follow(String),
    Apply(String),
}

pub(crate) fn resolve(index: &mut Index, roots: Vec<String>) {
    let mut pending: Vec<_> = roots.into_iter().map(Visit::Follow).collect();
    let mut seen = BTreeSet::new();
    while let Some(visit) = pending.pop() {
        match visit {
            Visit::Follow(name) => {
                let key = name.to_ascii_lowercase();
                let Some(class) = index.classes.get(&key) else {
                    continue;
                };
                if class.documented.type_imports.is_empty() || !seen.insert(key.clone()) {
                    continue;
                }
                pending.push(Visit::Apply(key));
                pending.extend(
                    class
                        .documented
                        .type_imports
                        .iter()
                        .map(|import| Visit::Follow(import.source.clone())),
                );
            }
            Visit::Apply(key) => apply_imports(index, &key),
        }
    }
}

fn apply_imports(index: &mut Index, key: &str) {
    let imports = &index.classes[key].documented.type_imports;
    let aliases: BTreeMap<_, _> = imports
        .iter()
        .filter_map(|import| {
            let source = index.class(&import.source)?;
            let ty = source.documented.templates.aliases.get(&import.original)?;
            if matches!(ty, ValueType::Template(template) if template.bound == ValueType::Mixed) {
                return None;
            }
            let ty = ty.substitute(
                &BTreeMap::new(),
                Some(RelativeTypes {
                    receiver: &ValueType::Class(source.name.clone()),
                    owner: &source.name,
                    parent: source.parents.first().map(String::as_str),
                }),
            );
            Some((import.local.clone(), ty))
        })
        .collect();
    if aliases.is_empty() {
        return;
    }
    let Some(class) = index.classes.get_mut(key) else {
        return;
    };
    let docs = &mut class.documented;
    rewrite(
        docs.templates
            .parameters
            .values_mut()
            .chain(docs.templates.aliases.values_mut())
            .chain(docs.generic_bases.iter_mut())
            .chain(docs.property_types.values_mut()),
        &aliases,
    );
    docs.templates.aliases.extend(aliases.clone());
    for property in class.properties.values_mut() {
        rewrite(property.documented_type.iter_mut(), &aliases);
    }
    for method in class.methods.values_mut() {
        apply_signature(method, &aliases);
    }
    for method in docs.method_types.values_mut() {
        apply_signature(Arc::make_mut(method), &aliases);
    }
}

fn apply_signature(signature: &mut Signature, aliases: &BTreeMap<String, ValueType>) {
    rewrite(
        signature
            .parameters
            .iter_mut()
            .map(|param| &mut param.ty)
            .chain([&mut signature.returns]),
        aliases,
    );
    let Some(doc) = &mut signature.documentation else {
        return;
    };
    let doc = Arc::make_mut(doc);
    let mut filtered;
    let aliases = if doc
        .template_names
        .iter()
        .any(|name| aliases.contains_key(name))
    {
        filtered = aliases.clone();
        for name in &doc.template_names {
            filtered.remove(name);
        }
        &filtered
    } else {
        aliases
    };
    rewrite(
        doc.params
            .values_mut()
            .chain(doc.variables.values_mut())
            .chain(doc.returns.iter_mut())
            .chain(doc.bases.iter_mut())
            .chain(doc.properties.values_mut())
            .chain(doc.methods.values_mut())
            .map(|annotation| &mut annotation.ty)
            .chain(doc.assertions.iter_mut().map(|assertion| &mut assertion.ty)),
        aliases,
    );
}

fn rewrite<'a>(
    types: impl Iterator<Item = &'a mut ValueType>,
    aliases: &BTreeMap<String, ValueType>,
) {
    for ty in types {
        *ty = ty.substitute(aliases, None);
    }
}
