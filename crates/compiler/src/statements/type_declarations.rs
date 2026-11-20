//! `function f(int&Stringable $value) {}` fails when the declaration is compiled.
//!
//! Validate declared type expressions before lowering discards their spelling
//! and scope. A union is a list of alternatives, each with one or more bounds.

use std::{borrow::Cow, collections::HashSet, ops::Range};

use phera_ast::{Name, NameKind, Type, TypeKind};

use crate::{CompileError, Compiler, policies};

pub(super) enum TypeSite<'a> {
    Parameter,
    Return,
    Property { class: &'a str, name: &'a str },
    Constant { class: &'a str, name: &'a str },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Bound {
    Standalone,
    Union,
    Intersection,
}

#[derive(Clone, Copy)]
pub(super) enum TypeScope<'a> {
    Global,
    Deferred,
    Class {
        name: &'a str,
        parent: Option<&'a str>,
    },
}

pub(super) fn builtin_type(lower: &str) -> bool {
    matches!(
        lower,
        "int"
            | "float"
            | "string"
            | "bool"
            | "array"
            | "object"
            | "iterable"
            | "callable"
            | "mixed"
            | "void"
            | "never"
            | "null"
            | "true"
            | "false"
    )
}

impl Compiler<'_> {
    pub(super) fn validate_declared_type(
        &self,
        ty: &Type,
        site: TypeSite<'_>,
        scope: TypeScope<'_>,
    ) -> Result<(), CompileError> {
        if let TypeKind::Named(name) = &ty.kind {
            let atom = self.type_bound(name, scope, Bound::Standalone)?;
            return self.validate_type_site(ty, &site, std::slice::from_ref(&atom));
        }
        let error = |message| self.error(message, self.lines.line(ty.span.start));
        let mut atoms = Vec::new();
        let mut alternatives = Vec::<Range<usize>>::new();
        let mut single = HashSet::new();
        let (options, nullable) = match &ty.kind {
            TypeKind::Union(items) => (items.as_slice(), false),
            TypeKind::Nullable(inner) => (std::slice::from_ref(inner.as_ref()), true),
            _ => (std::slice::from_ref(ty), false),
        };
        for option in options {
            let start = atoms.len();
            let (bounds, intersection) = match &option.kind {
                TypeKind::Intersection(items) => (items.as_slice(), true),
                _ => (std::slice::from_ref(option), false),
            };
            for bound in bounds {
                let TypeKind::Named(name) = &bound.kind else {
                    continue;
                };
                let form = if intersection {
                    Bound::Intersection
                } else {
                    Bound::Union
                };
                let atom = self.type_bound(name, scope, form)?;
                let identity = atom.to_ascii_lowercase();
                if intersection
                    && atoms[start..]
                        .iter()
                        .any(|other: &Cow<'_, str>| other.eq_ignore_ascii_case(&atom))
                {
                    return Err(error(format!("Duplicate type {atom} is redundant")));
                }
                if !intersection {
                    match identity.as_str() {
                        "void" => return Err(error("Void can only be used as a standalone type".into())),
                        "never" => return Err(error("never can only be used as a standalone type".into())),
                        "mixed" if nullable => return Err(error("Type mixed cannot be marked as nullable since mixed already includes null".into())),
                        "mixed" => return Err(error("Type mixed can only be used as a standalone type".into())),
                        "null" if nullable => return Err(error("null cannot be marked as nullable".into())),
                        _ => {}
                    }
                    let duplicate = if identity == "iterable" {
                        ["array", "traversable"]
                            .into_iter()
                            .find(|part| single.contains(*part))
                            .map(|part| {
                                if part == "array" {
                                    "array"
                                } else {
                                    "Traversable"
                                }
                            })
                    } else {
                        None
                    };
                    if let Some(duplicate) = duplicate {
                        return Err(error(format!("Duplicate type {duplicate} is redundant")));
                    }
                    if !single.insert(identity.clone()) {
                        return Err(error(format!("Duplicate type {atom} is redundant")));
                    }
                    if identity == "iterable" {
                        single.insert("array".to_owned());
                        single.insert("traversable".to_owned());
                    }
                }
                atoms.push(atom);
            }
            alternatives.push(start..atoms.len());
        }
        if single.contains("bool") {
            for literal in ["false", "true"] {
                if single.contains(literal) {
                    return Err(error(format!("Duplicate type {literal} is redundant")));
                }
            }
        } else if single.contains("true") && single.contains("false") {
            return Err(error(
                "Type contains both true and false, bool must be used instead".into(),
            ));
        }
        if single.contains("object") && atoms.iter().any(|atom| !builtin_type(atom)) {
            let display = declaration_union(&atoms, &alternatives, nullable);
            return Err(error(format!(
                "Type {display} contains both object and a class type, which is redundant"
            )));
        }
        for (index, range) in alternatives
            .iter()
            .enumerate()
            .filter(|(_, range)| range.len() > 1)
        {
            let group = &atoms[range.clone()];
            for (other_index, other) in alternatives.iter().enumerate() {
                if other_index == index
                    || other.len() > group.len()
                    || (other.len() == group.len() && other_index < index)
                {
                    continue;
                }
                let other = &atoms[other.clone()];
                if other
                    .iter()
                    .all(|bound| group.iter().any(|name| name.eq_ignore_ascii_case(bound)))
                {
                    let display = group.join("&");
                    let other_display = other.join("&");
                    let message = if other.len() == group.len() {
                        format!("Type {other_display} is redundant with type {display}")
                    } else {
                        format!(
                            "Type {display} is redundant as it is more restrictive than type {other_display}"
                        )
                    };
                    return Err(error(message));
                }
            }
        }
        self.validate_type_site(ty, &site, &atoms)
    }

    fn type_bound<'a>(
        &self,
        name: &'a Name,
        scope: TypeScope<'a>,
        form: Bound,
    ) -> Result<Cow<'a, str>, CompileError> {
        let lower = lowercase(&name.text);
        let error = |message| self.error(message, self.lines.line(name.span.start));
        let relative = matches!(lower.as_ref(), "self" | "parent" | "static");
        if name.kind != NameKind::Unqualified && (builtin_type(&lower) || lower == "static") {
            return Err(error(format!(
                "Type declaration '{}' must be unqualified",
                lower
            )));
        }
        if name.kind == NameKind::Unqualified && relative {
            let resolved = match (scope, lower.as_ref()) {
                (TypeScope::Global, _) => {
                    return Err(error(format!(
                        "Cannot use \"{lower}\" when no class scope is active"
                    )));
                }
                (TypeScope::Class { name, .. }, "self") => Some(name),
                (
                    TypeScope::Class {
                        parent: Some(parent),
                        ..
                    },
                    "parent",
                ) => Some(parent),
                (TypeScope::Class { parent: None, .. }, "parent") => {
                    return Err(error(
                        "Cannot use \"parent\" when current class scope has no parent".into(),
                    ));
                }
                _ => None,
            };
            if form == Bound::Intersection && resolved.is_none() {
                return Err(error(format!(
                    "Type {} cannot be part of an intersection type",
                    name.text
                )));
            }
            return Ok(resolved.map_or(lower, Cow::Borrowed));
        }
        if builtin_type(&lower) {
            if form == Bound::Intersection {
                let display = if lower == "iterable" {
                    "Traversable|array"
                } else {
                    &lower
                };
                return Err(error(format!(
                    "Type {display} cannot be part of an intersection type"
                )));
            }
            return Ok(lower);
        }
        if let Some((_, part)) = lower.rsplit_once('\\')
            && (builtin_type(part) || matches!(part, "self" | "parent" | "static"))
        {
            let resolved = self.resolve(name, policies::NameKind::Class).0;
            return Err(error(format!(
                "Cannot use \"{resolved}\" as a type name as it is reserved"
            )));
        }
        Ok(match form {
            Bound::Standalone => Cow::Borrowed(&name.text),
            _ => Cow::Owned(self.resolve(name, policies::NameKind::Class).0),
        })
    }

    fn validate_type_site(
        &self,
        ty: &Type,
        site: &TypeSite<'_>,
        atoms: &[Cow<'_, str>],
    ) -> Result<(), CompileError> {
        let invalid = atoms
            .iter()
            .find(|atom| matches!(atom.as_ref(), "void" | "never" | "callable"));
        let message = match (site, invalid) {
            (TypeSite::Parameter, Some(atom)) if atom != "callable" => {
                format!("{atom} cannot be used as a parameter type")
            }
            (TypeSite::Property { class, name }, Some(_)) => {
                let ty = phera_bytecode::union_in_php_order(&self.declaration_type(ty));
                format!("Property {class}::${name} cannot have type {ty}")
            }
            (TypeSite::Constant { class, name }, Some(_)) => {
                let ty = phera_bytecode::union_in_php_order(&self.declaration_type(ty));
                format!("Class constant {class}::{name} cannot have type {ty}")
            }
            _ => return Ok(()),
        };
        Err(self.error(message, self.lines.line(ty.span.start)))
    }
}

fn lowercase(name: &str) -> Cow<'_, str> {
    if name.bytes().any(|byte| byte.is_ascii_uppercase()) {
        Cow::Owned(name.to_ascii_lowercase())
    } else {
        Cow::Borrowed(name)
    }
}

fn declaration_union(
    atoms: &[Cow<'_, str>],
    alternatives: &[Range<usize>],
    nullable: bool,
) -> String {
    let mut members = Vec::new();
    let mut has_iterable = false;
    for range in alternatives {
        let bounds = &atoms[range.clone()];
        match bounds {
            [atom] if atom == "iterable" => {
                members.push("Traversable".to_owned());
                has_iterable = true;
            }
            [atom] => members.push(atom.to_string()),
            _ => members.push(bounds.join("&")),
        }
    }
    if has_iterable {
        members.push("array".to_owned());
    }
    if nullable {
        members.push("null".to_owned());
    }
    phera_bytecode::union_in_php_order(&members.join("|"))
}
