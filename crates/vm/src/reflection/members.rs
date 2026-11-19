//! `ReflectionClass::getProperties()` keeps declaration order and omits inherited private members.

use std::collections::HashSet;

use phera_bytecode::reflection::DeclarationMetadata;
use phera_bytecode::{ClassDeclaration, CompiledUnit, TraitAdaptation, Visibility};
use phera_extension_api::reflection::{AttributeTarget, MemberInfo, PropertyHandle, PropertySlot};

use crate::classes::class::ConstantInitializer;
use crate::{Vm, classes::Class, code_table::FunctionIndex};

use super::{flags, source_info, visibility_bits};

impl Vm {
    pub(super) fn reflection_trait_aliases(
        &self,
        class: &Class,
        declaration: &ClassDeclaration,
    ) -> Vec<(String, String)> {
        declaration
            .traits
            .iter()
            .flat_map(|usage| &usage.adaptations)
            .filter_map(|adaptation| {
                let TraitAdaptation::Alias {
                    trait_name,
                    method,
                    alias: Some(alias),
                    ..
                } = adaptation
                else {
                    return None;
                };
                let provider = if let Some(trait_name) = trait_name {
                    self.classes.get(trait_name)
                } else {
                    let key = method.to_ascii_lowercase();
                    class
                        .traits
                        .iter()
                        .filter_map(|name| self.classes.get(name))
                        .find(|provider| provider.methods.contains_key(&key))
                }?;
                Some((alias.clone(), format!("{}::{method}", provider.name)))
            })
            .collect()
    }

    /// Source metadata for a composed member belongs to its original trait.
    /// Walk each directly used trait in order, then its nested traits.
    fn member_declarations<'a>(
        &'a self,
        class: &Class,
    ) -> Vec<(FunctionIndex, &'a CompiledUnit, &'a ClassDeclaration)> {
        let compiled = |(first, index): (FunctionIndex, phera_bytecode::ClassId)| {
            let unit = self.code.unit(first);
            unit.classes
                .get(index.index())
                .map(|declaration| (first, unit.as_ref(), declaration))
        };
        let mut sources: Vec<_> = class.declaration.and_then(compiled).into_iter().collect();
        let mut pending: Vec<_> = class
            .traits
            .iter()
            .rev()
            .filter_map(|name| self.classes.get(name))
            .collect();
        let mut visited = HashSet::new();
        while let Some(provider) = pending.pop() {
            if visited.insert(provider.index) {
                sources.extend(provider.declaration.and_then(compiled));
                pending.extend(
                    provider
                        .traits
                        .iter()
                        .rev()
                        .filter_map(|name| self.classes.get(name)),
                );
            }
        }
        sources
    }

    fn reflect_member_source(
        &self,
        info: &mut MemberInfo,
        first: FunctionIndex,
        unit: &CompiledUnit,
        metadata: &DeclarationMetadata,
        target: AttributeTarget,
    ) {
        info.source = Some(source_info(unit, metadata));
        info.attributes =
            self.reflection_attributes(first, &metadata.attributes, target, Some(&info.class));
    }

    pub(super) fn reflection_methods(&self, class: &Class) -> Vec<MemberInfo> {
        let mut methods: Vec<_> = class
            .methods
            .values()
            .filter(|method| {
                method.visibility != Visibility::Private || method.declaring == class.name
            })
            .collect();
        methods.sort_by_key(|method| self.method_listing_order(class, method));
        methods
            .into_iter()
            .map(|method| MemberInfo {
                name: method.name.clone(),
                class: method.declaring.clone(),
                lookup_class: class.name.clone(),
                modifiers: visibility_bits(method.visibility)
                    | flags(method.is_static, 16)
                    | flags(method.is_final, 32)
                    | flags(method.is_abstract, 64),
                ..MemberInfo::default()
            })
            .collect()
    }

    pub(super) fn reflection_properties(&self, class: &Class) -> Vec<MemberInfo> {
        let mut members = Vec::new();
        let mut scope = Some(class);
        while let Some(owner) = scope {
            let declarations = self.member_declarations(owner);
            let property_source = |name: &str| {
                declarations
                    .iter()
                    .enumerate()
                    .find_map(|(index, (first, unit, declaration))| {
                        declaration
                            .properties
                            .iter()
                            .enumerate()
                            .find(|(_, candidate)| unit.names[candidate.name.index()] == name)
                            .map(|(position, property)| {
                                ((index, position), *first, *unit, property)
                            })
                    })
            };
            let mut properties: Vec<_> = class
                .defaults
                .iter()
                .enumerate()
                .map(|(index, property)| (PropertySlot::Instance(index), property))
                .chain(
                    class
                        .static_properties
                        .iter()
                        .enumerate()
                        .map(|(index, property)| (PropertySlot::Static(index), property)),
                )
                .filter(|(_, property)| {
                    property.declaring == owner.name
                        && (property.visibility != Visibility::Private || owner.name == class.name)
                })
                .collect();
            if owner.declaration.is_some() {
                properties.sort_by_key(|(_, property)| {
                    property_source(&property.name)
                        .map_or((usize::MAX, usize::MAX), |(order, ..)| order)
                });
            } else if let Some(entry) = self.registry.get_class(&owner.name) {
                properties.sort_by_key(|(_, property)| {
                    entry
                        .properties
                        .iter()
                        .position(|candidate| candidate.name == property.name)
                });
            }
            for (slot, property) in properties {
                let is_virtual = property
                    .hooks
                    .as_ref()
                    .is_some_and(|hooks| hooks.hooks.is_virtual);
                let is_abstract = property
                    .hooks
                    .as_ref()
                    .is_some_and(|hooks| hooks.methods().any(|hook| hook.is_abstract));
                let mut info = MemberInfo {
                    name: property.name.clone(),
                    class: property.declaring.clone(),
                    lookup_class: class.name.clone(),
                    modifiers: visibility_bits(property.visibility)
                        | flags(property.static_storage.is_some(), 16)
                        | flags(property.is_readonly, 128)
                        | flags(property.is_final, 32)
                        | flags(is_virtual, 512)
                        | flags(is_abstract, 64)
                        | flags(
                            property.set_visibility == Some(Visibility::Protected)
                                || property.is_readonly
                                    && property.visibility == Visibility::Public
                                    && property.set_visibility.is_none(),
                            2048,
                        )
                        | flags(property.set_visibility == Some(Visibility::Private), 4096),
                    declared_type: property.ty.as_ref().map(|ty| ty.display.clone()),
                    has_default: property.initialized && !is_virtual,
                    property: Some(PropertyHandle {
                        class: class.index.number(),
                        slot,
                    }),
                    ..MemberInfo::default()
                };
                if let Some((_, first, unit, declared)) = property_source(&property.name) {
                    info.promoted = declared.promoted;
                    self.reflect_member_source(
                        &mut info,
                        first,
                        unit,
                        &declared.metadata,
                        AttributeTarget::Property,
                    );
                } else if owner.enum_info.is_some() {
                    info.has_default = false;
                    info.declared_type = Some(
                        if property.name == "name" {
                            "string"
                        } else {
                            match owner.enum_info.as_ref().and_then(|info| info.backing) {
                                Some(phera_bytecode::EnumBacking::Int) => "int",
                                _ => "string",
                            }
                        }
                        .into(),
                    );
                    info.modifiers |= 2048;
                }
                members.push(info);
            }
            scope = owner.parent.as_deref();
        }
        members
    }

    pub(super) fn reflection_constants(&self, class: &Class) -> Vec<MemberInfo> {
        let mut members = Vec::new();
        let interfaces = crate::class_listings::interfaces_in_php_order(class);
        let ancestors = std::iter::successors(Some(class), |owner| owner.parent.as_deref());
        for owner in ancestors.chain(interfaces) {
            let declarations = self.member_declarations(owner);
            let constant_source = |name: &str, is_case: bool| {
                declarations
                    .iter()
                    .enumerate()
                    .find_map(|(index, (first, unit, declaration))| {
                        let (position, metadata) = if is_case {
                            declaration
                                .enum_cases
                                .iter()
                                .enumerate()
                                .find(|(_, candidate)| unit.names[candidate.name.index()] == name)
                                .map(|(position, case)| (position, &case.metadata))
                        } else {
                            declaration
                                .constants
                                .iter()
                                .enumerate()
                                .find(|(_, candidate)| unit.names[candidate.name.index()] == name)
                                .map(|(position, constant)| (position, &constant.metadata))
                        }?;
                        Some((
                            (index, metadata.span.start, position),
                            *first,
                            *unit,
                            metadata,
                        ))
                    })
            };
            let mut constants: Vec<_> = class
                .constants
                .iter()
                .filter(|(_, constant)| {
                    constant.declaring == owner.name
                        && (constant.visibility != Visibility::Private || owner.name == class.name)
                })
                .collect();
            let is_case = |initializer: &ConstantInitializer| {
                matches!(initializer, ConstantInitializer::EnumCase { .. })
            };
            if owner.declaration.is_some() {
                constants.sort_by_key(|(name, constant)| {
                    constant_source(name, is_case(&constant.initializer))
                        .map_or((usize::MAX, usize::MAX, usize::MAX), |(order, ..)| order)
                });
            } else if let Some(entry) = self.registry.get_class(&owner.name) {
                constants.sort_by_key(|(name, _)| {
                    entry
                        .constants
                        .iter()
                        .position(|(candidate, _)| candidate == *name)
                });
            }
            for (name, constant) in constants {
                let mut info = MemberInfo {
                    name: name.clone(),
                    class: constant.declaring.clone(),
                    lookup_class: class.name.clone(),
                    modifiers: visibility_bits(constant.visibility) | flags(constant.is_final, 32),
                    declared_type: constant.ty.as_ref().map(|ty| ty.display.clone()),
                    enum_case: is_case(&constant.initializer),
                    ..MemberInfo::default()
                };
                if let Some((_, first, unit, metadata)) = constant_source(name, info.enum_case) {
                    self.reflect_member_source(
                        &mut info,
                        first,
                        unit,
                        metadata,
                        AttributeTarget::ClassConstant,
                    );
                }
                members.push(info);
            }
        }
        members
    }
}
