//! Index class members: `class C { public int $x; function f(string $s): void {} }`.
//! Promoted properties, enum cases and trait contracts retain their source locations.

use crate::collect::uses_argument_introspection;
use crate::{index::*, names::Names, types::ValueType};
use phera_ast::*;
use std::collections::BTreeMap;

pub fn class_info(
    class: &ClassDeclaration,
    names: &Names,
    source: &str,
    location: &impl Fn(usize) -> Location,
    read_docs: bool,
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
        enum_cases: Default::default(),
        location: location(class.span.start),
        open_properties: false,
        mapped_repository: crate::framework_knowledge::doctrine::mapped_repository(
            &class.attributes,
            names,
        ),
        documented: crate::phpdoc::text(source, class.doc_comment)
            .map_or_else(Default::default, |doc| crate::phpdoc::members(doc, names)),
    };
    if read_docs && class.doc_comment.is_some() {
        let doc = crate::phpdoc::read(source, class.doc_comment, names, &Default::default());
        info.documented.templates = doc.templates;
        info.documented.template_names = doc.template_names;
        info.documented.method_types = doc
            .methods
            .into_iter()
            .filter_map(|(name, annotation)| {
                if let ValueType::Callable(signature) = annotation.ty {
                    Some((name, signature))
                } else {
                    None
                }
            })
            .collect();
        info.documented.property_types = doc
            .properties
            .into_iter()
            .map(|(name, annotation)| (name, annotation.ty))
            .collect();
        info.documented.generic_bases = doc
            .bases
            .into_iter()
            .map(|annotation| annotation.ty)
            .collect();
    }
    for member in &class.members {
        add_member(&mut info, member, names, source, location, read_docs);
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
    source: &str,
    location: &impl Fn(usize) -> Location,
    read_docs: bool,
) {
    match &member.kind {
        ClassMemberKind::Method(method) => {
            let doc = crate::phpdoc::text(source, member.doc_comment);
            let documentation = (read_docs && member.doc_comment.is_some()).then(|| {
                crate::phpdoc::read(
                    source,
                    member.doc_comment,
                    names,
                    &info.documented.templates,
                )
            });
            add_method(info, method, names, doc, member.span.start, location);
            if read_docs {
                for parameter in method
                    .parameters
                    .iter()
                    .filter(|parameter| parameter.is_promoted())
                {
                    let parameter_docs = crate::phpdoc::read(
                        source,
                        parameter.doc_comment,
                        names,
                        &info.documented.templates,
                    );
                    if let Some(property) = info.properties.get_mut(&parameter.name) {
                        property.documented_type = parameter_docs
                            .variables
                            .get(&None)
                            .or_else(|| parameter_docs.variables.get(&Some(parameter.name.clone())))
                            .or_else(|| documentation.as_ref()?.params.get(&parameter.name))
                            .map(|annotation| annotation.ty.clone());
                    }
                }
            }
            if let Some(documentation) = documentation
                && let Some(signature) =
                    info.methods.get_mut(&method.name.name.to_ascii_lowercase())
            {
                signature.documentation = Some(std::sync::Arc::new(documentation));
            }
        }
        ClassMemberKind::Property {
            modifiers,
            ty,
            items,
            ..
        } => {
            let documented = (read_docs && member.doc_comment.is_some()).then(|| {
                crate::phpdoc::read(
                    source,
                    member.doc_comment,
                    names,
                    &info.documented.templates,
                )
            });
            for p in items {
                info.properties.insert(
                    p.name.clone(),
                    PropertyInfo {
                        ty: ValueType::declared(ty.as_ref(), names),
                        documented_type: documented.as_ref().and_then(|doc| {
                            doc.variables
                                .get(&Some(p.name.clone()))
                                .or_else(|| doc.variables.get(&None))
                                .map(|annotation| annotation.ty.clone())
                        }),
                        default: match p.default.as_ref().map(|value| &value.kind) {
                            Some(ExpressionKind::Literal(Literal::Null)) => {
                                PropertyDefault::ExplicitNull
                            }
                            None if ty.is_none() && !p.has_hook_list => {
                                PropertyDefault::ImplicitNull
                            }
                            _ => PropertyDefault::Other,
                        },
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
            info.enum_cases.insert(name.name.clone());
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
    doc: Option<&str>,
    offset: usize,
    location: &impl Fn(usize) -> Location,
) {
    let mut signature = Signature::new(
        &method.parameters,
        method.return_type.as_ref(),
        names,
        location(offset),
    );
    signature.apply_phpdoc(doc, names);
    signature.modifiers = method.modifiers;
    signature.returns_reference = method.by_reference;
    signature.global_effects = method
        .body
        .as_ref()
        .map_or(crate::global_effects::GlobalEffects::Unknown, |body| {
            crate::global_effects::of(body)
        });
    signature.abstract_body = method.body.is_none() || info.kind == ClassKind::Interface;
    signature.accepts_extra = method
        .body
        .as_ref()
        .is_some_and(|body| uses_argument_introspection(body));
    info.methods
        .insert(method.name.name.to_ascii_lowercase(), signature);
    for p in &method.parameters {
        if p.is_promoted() {
            info.properties.insert(
                p.name.clone(),
                PropertyInfo {
                    ty: ValueType::declared(p.ty.as_ref(), names),
                    documented_type: None,
                    default: crate::index::PropertyDefault::Other,
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
                ..
            } => MemberAdaptation::Alias {
                source: trait_name
                    .as_ref()
                    .map(|n| names.resolve(n, UseKind::Class)[0].clone()),
                method: method.name.to_ascii_lowercase(),
                alias: alias.as_ref().unwrap_or(method).name.to_ascii_lowercase(),
                visibility: *visibility,
            },
            TraitAdaptation::Precedence {
                trait_name, method, ..
            } => MemberAdaptation::Precedence {
                source: names.resolve(trait_name, UseKind::Class)[0].clone(),
                method: method.name.to_ascii_lowercase(),
            },
        });
    }
}
