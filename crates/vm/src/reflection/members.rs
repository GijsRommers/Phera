//! `ReflectionClass::getProperties()` keeps declaration order and omits inherited private members.

use phera_bytecode::Visibility;
use phera_extension_api::reflection::{AttributeTarget, MemberInfo, PropertyHandle, PropertySlot};

use crate::{Vm, classes::Class};

use super::{flags, source_info, visibility_bits};

impl Vm {
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
            let declaration = owner.declaration.and_then(|(first, index)| {
                let unit = self.code.unit(first);
                unit.classes
                    .get(index.index())
                    .map(|declaration| (first, unit, declaration))
            });
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
            if let Some((_, unit, declaration)) = declaration {
                properties.sort_by_key(|(_, property)| {
                    declaration
                        .properties
                        .iter()
                        .position(|candidate| unit.names[candidate.name.index()] == property.name)
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
                let mut info = MemberInfo {
                    name: property.name.clone(),
                    class: property.declaring.clone(),
                    lookup_class: class.name.clone(),
                    modifiers: visibility_bits(property.visibility)
                        | flags(property.static_storage.is_some(), 16)
                        | flags(property.is_readonly, 128)
                        | flags(property.is_final, 32)
                        | flags(property.set_visibility == Some(Visibility::Protected), 2048)
                        | flags(property.set_visibility == Some(Visibility::Private), 4096),
                    declared_type: property.ty.as_ref().map(|ty| ty.display.clone()),
                    has_default: property.initialized,
                    property: Some(PropertyHandle {
                        class: class.index.number(),
                        slot,
                    }),
                    ..MemberInfo::default()
                };
                if let Some((first, unit, declaration)) = declaration
                    && let Some(declared) = declaration
                        .properties
                        .iter()
                        .find(|candidate| unit.names[candidate.name.index()] == property.name)
                {
                    info.promoted = declared.promoted;
                    info.source = Some(source_info(unit, &declared.metadata));
                    info.attributes = self.reflection_attributes(
                        first,
                        &declared.metadata.attributes,
                        AttributeTarget::Property,
                        Some(&owner.name),
                    );
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
            let declaration = owner.declaration.and_then(|(first, index)| {
                let unit = self.code.unit(first);
                unit.classes
                    .get(index.index())
                    .map(|declaration| (first, unit, declaration))
            });
            let mut constants: Vec<_> = class
                .constants
                .iter()
                .filter(|(_, constant)| {
                    constant.declaring == owner.name
                        && (constant.visibility != Visibility::Private || owner.name == class.name)
                })
                .collect();
            if let Some((_, unit, declaration)) = declaration {
                constants.sort_by_key(|(name, _)| {
                    declaration
                        .constants
                        .iter()
                        .position(|candidate| &unit.names[candidate.name.index()] == *name)
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
                    ..MemberInfo::default()
                };
                if let Some((first, unit, declaration)) = declaration
                    && let Some(declared) = declaration
                        .constants
                        .iter()
                        .find(|candidate| &unit.names[candidate.name.index()] == name)
                {
                    info.source = Some(source_info(unit, &declared.metadata));
                    info.attributes = self.reflection_attributes(
                        first,
                        &declared.metadata.attributes,
                        AttributeTarget::ClassConstant,
                        Some(&owner.name),
                    );
                }
                members.push(info);
            }
        }
        members
    }
}
