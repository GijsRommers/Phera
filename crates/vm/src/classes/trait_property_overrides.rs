//! A trait's `#[Override] public $property` is validated in each consuming class.

use std::{collections::HashSet, rc::Rc};

use phera_bytecode::{ClassDeclaration, CompiledUnit};
use phera_extension_api::NativeError;

use super::{
    Class,
    link_members::{overrides_property, property_override_error},
};
use crate::Vm;

impl Vm {
    pub(super) fn check_trait_property_overrides(
        &self,
        declaration: &ClassDeclaration,
        unit: &CompiledUnit,
        traits: &[Rc<Class>],
        parent: Option<&Class>,
        interfaces: &[Rc<Class>],
    ) -> Result<(), NativeError> {
        let mut pending: Vec<_> = traits.to_vec();
        let mut seen = HashSet::new();
        while let Some(provider) = pending.pop() {
            if !seen.insert(provider.index) {
                continue;
            }
            pending.extend(
                provider
                    .traits
                    .iter()
                    .filter_map(|name| self.classes.get(name)),
            );
            let Some((first, index)) = provider.declaration else {
                continue;
            };
            let source = self.code.unit(first);
            if !source.has_link_attributes {
                continue;
            }
            for property in &source.classes[index.index()].properties {
                let name = &source.names[property.name.index()];
                if crate::attribute_diagnostics::attribute(
                    &property.metadata.attributes,
                    "Override",
                )
                .is_none()
                    || declaration
                        .properties
                        .iter()
                        .any(|direct| unit.names[direct.name.index()] == *name)
                {
                    continue;
                }
                let inherited = parent.map_or(&[][..], |parent| {
                    if property.is_static {
                        &parent.static_properties
                    } else {
                        &parent.defaults
                    }
                });
                if !overrides_property(name, &declaration.name, inherited, interfaces) {
                    return Err(property_override_error(
                        declaration,
                        unit,
                        name,
                        declaration.metadata.span.start,
                    ));
                }
            }
        }
        Ok(())
    }
}
