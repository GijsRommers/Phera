//! Index class members: `class C { public int $x; function f(string $s): void {} }`.
//! Promoted properties, enum cases and trait contracts retain their source locations.

use crate::collect::uses_argument_introspection;
use crate::{index::*, names::Names, types::ValueType};
use phera_ast::*;
use std::collections::BTreeMap;

pub fn class_info(
    class: &ClassDeclaration,
    names: &Names,
    location: &impl Fn(usize) -> Location,
) -> ClassInfo {
    let resolve = |n: &Name| names.resolve(n, UseKind::Class)[0].clone();
    let mut info = ClassInfo {
        name: names.declaration(class.name.as_ref().map_or("@anonymous", |n| &n.name)),
        kind: class.kind,
        modifiers: class.modifiers,
        parents: class.extends.iter().map(resolve).collect(),
        interfaces: class.implements.iter().map(resolve).collect(),
        traits: Vec::new(),
        adaptations: Vec::new(),
        methods: BTreeMap::new(),
        properties: BTreeMap::new(),
        constants: BTreeMap::new(),
        location: location(class.span.start),
        open_properties: false,
    };
    for member in &class.members {
        add_member(&mut info, member, names, location);
    }
    crate::enums::add_members(&mut info, class, names);
    info.open_properties = class
        .attributes
        .iter()
        .any(|a| a.name.text.eq_ignore_ascii_case("AllowDynamicProperties"));
    info
}

fn add_member(
    info: &mut ClassInfo,
    member: &ClassMember,
    names: &Names,
    location: &impl Fn(usize) -> Location,
) {
    match &member.kind {
        ClassMemberKind::Method(method) => {
            add_method(info, method, names, member.span.start, location)
        }
        ClassMemberKind::Property {
            modifiers,
            ty,
            items,
            ..
        } => {
            for p in items {
                info.properties.insert(
                    p.name.clone(),
                    PropertyInfo {
                        ty: ValueType::declared(ty.as_ref(), names),
                        modifiers: *modifiers,
                        location: location(p.span.start),
                    },
                );
            }
        }
        ClassMemberKind::Constant {
            modifiers,
            ty,
            items,
            ..
        } => add_constants(info, *modifiers, ty.as_ref(), items, names, location),
        ClassMemberKind::EnumCase { name, .. } => {
            info.constants.insert(
                name.name.clone(),
                ConstantInfo {
                    ty: ValueType::Class(info.name.clone()),
                    modifiers: Modifiers::default(),
                    location: location(name.span.start),
                },
            );
        }
        ClassMemberKind::TraitUse {
            traits,
            adaptations,
        } => add_traits(info, traits, adaptations, names),
    }
}

fn add_method(
    info: &mut ClassInfo,
    method: &Method,
    names: &Names,
    offset: usize,
    location: &impl Fn(usize) -> Location,
) {
    let mut signature = Signature::new(
        &method.parameters,
        method.return_type.as_ref(),
        names,
        location(offset),
    );
    signature.modifiers = method.modifiers;
    signature.abstract_body = method.body.is_none() || info.kind == ClassKind::Interface;
    signature.accepts_extra = method
        .body
        .as_ref()
        .is_some_and(|body| uses_argument_introspection(body));
    info.methods
        .insert(method.name.name.to_lowercase(), signature);
    for p in &method.parameters {
        if p.modifiers.visibility.is_some() {
            info.properties.insert(
                p.name.clone(),
                PropertyInfo {
                    ty: ValueType::declared(p.ty.as_ref(), names),
                    modifiers: p.modifiers,
                    location: location(p.span.start),
                },
            );
        }
    }
}

fn add_constants(
    info: &mut ClassInfo,
    modifiers: Modifiers,
    ty: Option<&Type>,
    items: &[ConstantDeclaration],
    names: &Names,
    location: &impl Fn(usize) -> Location,
) {
    for c in items {
        info.constants.insert(
            c.name.name.clone(),
            ConstantInfo {
                ty: ValueType::declared(ty, names),
                modifiers,
                location: location(c.span.start),
            },
        );
    }
}

fn add_traits(
    info: &mut ClassInfo,
    traits: &[Name],
    adaptations: &[TraitAdaptation],
    names: &Names,
) {
    info.traits.extend(
        traits
            .iter()
            .map(|n| names.resolve(n, UseKind::Class)[0].clone()),
    );
    for adaptation in adaptations {
        info.adaptations.push(match adaptation {
            TraitAdaptation::Alias {
                trait_name,
                method,
                alias,
                visibility,
            } => MemberAdaptation::Alias {
                source: trait_name
                    .as_ref()
                    .map(|n| names.resolve(n, UseKind::Class)[0].clone()),
                method: method.name.to_lowercase(),
                alias: alias.as_ref().unwrap_or(method).name.to_lowercase(),
                visibility: *visibility,
            },
            TraitAdaptation::Precedence {
                trait_name, method, ..
            } => MemberAdaptation::Precedence {
                source: names.resolve(trait_name, UseKind::Class)[0].clone(),
                method: method.name.to_lowercase(),
            },
        });
    }
}
