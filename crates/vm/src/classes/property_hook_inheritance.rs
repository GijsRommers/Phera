//! `interface I { public Animal $pet { get; } }` permits a readable Dog
//! property. Backed parent properties retain invariant types and storage.

use std::{rc::Rc, sync::Arc};

use phera_bytecode::{ClassDeclaration, ClassKind, CompiledUnit, PropertyHooks, Visibility};
use phera_extension_api::NativeError;

use super::{Class, class::PropertyDefault, property_hooks::LinkedPropertyHooks};
use crate::{Vm, code_table::FunctionIndex};

impl Vm {
    pub(super) fn verify_set_hook_type(
        &self,
        property: &PropertyDefault,
        unit: &CompiledUnit,
        line: usize,
    ) -> Result<(), NativeError> {
        let Some(hook) = property
            .hooks
            .as_ref()
            .and_then(|entry| entry.hooks.set.as_ref())
        else {
            return Ok(());
        };
        let function = self.code.function(hook.function);
        let Some((parameter, parameter_type)) = function.parameters.first().and_then(|parameter| {
            self.code
                .unit(hook.function)
                .types
                .get(parameter.ty?.index())
                .map(|ty| (parameter, ty))
        }) else {
            return Ok(());
        };
        let scope = &property.declaring;
        if self.type_subset(
            property_type(property),
            scope,
            Some(&parameter_type.display),
            scope,
            unit,
        ) {
            return Ok(());
        }
        let argument = self
            .code
            .unit(hook.function)
            .names
            .get(parameter.name.index())
            .map_or("value", String::as_str);
        let failure = crate::error_traces::DeclarationFailure {
            message: format!(
                "Type of parameter ${argument} of hook {scope}::${}::set must be compatible with property type",
                property.name
            ),
            file: unit.file.clone(),
            line,
        };
        let missing = self
            .unresolved_contract_types(property_type(property), Some(&parameter_type.display), unit)
            .map(str::to_owned);
        Err(crate::autoload::ClassMiss::property_signature(
            missing, failure,
        ))
    }

    pub(super) fn inherit_property_hooks(
        &mut self,
        property: &mut PropertyDefault,
        inherited: &PropertyDefault,
        unit: &CompiledUnit,
    ) -> Result<(), NativeError> {
        if inherited.is_final {
            return Err(NativeError::error(format!(
                "Cannot override final property {}::${}",
                inherited.declaring, inherited.name
            )));
        }
        if property.hooks.is_none() && inherited.hooks.is_none() {
            return Ok(());
        }
        self.check_property_hook_signatures(property, inherited, unit)?;
        self.check_property_type(property, inherited, unit)?;
        let virtual_property = is_virtual(property) && is_virtual(inherited);
        let mut linked =
            property
                .hooks
                .as_deref()
                .cloned()
                .unwrap_or_else(|| LinkedPropertyHooks {
                    hooks: PropertyHooks {
                        get: None,
                        set: None,
                        is_virtual: false,
                    },
                    prototype: Rc::from(inherited.declaring.as_str()),
                });
        linked.hooks.is_virtual = virtual_property;
        if let Some(parent) = &inherited.hooks {
            linked.prototype = parent.prototype.clone();
            for (operation, child, parent) in [
                ("get", &mut linked.hooks.get, &parent.hooks.get),
                ("set", &mut linked.hooks.set, &parent.hooks.set),
            ] {
                let Some(parent) = parent else { continue };
                if child.is_none()
                    && (!parent.is_abstract
                        || virtual_property
                        || (operation == "set" && property.is_readonly))
                {
                    *child = Some(parent.clone());
                }
            }
        }
        if linked.has_hooks() || inherited.hooks.is_some() {
            property.hooks = Some(Rc::new(linked));
        }
        if !virtual_property && property.ty.is_none() {
            property.initialized = true;
        }
        Ok(())
    }

    pub(super) fn check_property_type(
        &self,
        property: &PropertyDefault,
        inherited: &PropertyDefault,
        unit: &CompiledUnit,
    ) -> Result<(), NativeError> {
        let covariant = is_virtual(inherited)
            && inherited
                .hooks
                .as_ref()
                .is_some_and(|hooks| hooks.hooks.set.is_none());
        let contravariant = is_virtual(inherited)
            && inherited
                .hooks
                .as_ref()
                .is_some_and(|hooks| hooks.hooks.get.is_none());
        let expected = property_type(inherited);
        let actual = property_type(property);
        let class = &property.declaring;
        let parent = &inherited.declaring;
        let subset =
            |child: Option<&str>, child_scope: &str, parent: Option<&str>, parent_scope: &str| {
                child.is_some() == parent.is_some()
                    && self.type_subset(child, child_scope, parent, parent_scope, unit)
            };
        if (contravariant || subset(actual, class, expected, parent))
            && (covariant || subset(expected, parent, actual, class))
        {
            return Ok(());
        }
        let name = &property.name;
        let Some(expected) = expected else {
            return Err(NativeError::error(format!(
                "Type of {class}::${name} must be omitted to match the parent definition in class {parent}"
            )));
        };
        let variance = if covariant {
            "subtype of "
        } else if contravariant {
            "supertype of "
        } else {
            ""
        };
        Err(NativeError::error(format!(
            "Type of {class}::${name} must be {variance}{expected} (as in class {parent})"
        )))
    }

    pub(super) fn link_property_contracts(
        &mut self,
        declaration: &ClassDeclaration,
        unit: &Arc<CompiledUnit>,
        interfaces: &[Rc<Class>],
        defaults: &mut Vec<PropertyDefault>,
    ) -> Result<(), NativeError> {
        for interface in interfaces {
            for requirement in &interface.defaults {
                let Some(property) = defaults.iter_mut().find(|property| {
                    property.name == requirement.name && property.visibility == Visibility::Public
                }) else {
                    if let Some(property) = defaults
                        .iter()
                        .find(|property| property.name == requirement.name)
                    {
                        return Err(NativeError::error(format!(
                            "Access level to {}::${} must be public (as in class {})",
                            property.declaring, property.name, requirement.declaring
                        )));
                    }
                    defaults.push(requirement.clone());
                    continue;
                };
                if has_hook(requirement, "set") && property.is_readonly {
                    return Err(NativeError::error(format!(
                        "Set access level of {}::${} must be omitted (as in class {})",
                        property.declaring, property.name, requirement.declaring
                    )));
                }
                if has_hook(requirement, "get")
                    && is_virtual(property)
                    && !has_hook(property, "get")
                    || has_hook(requirement, "set")
                        && is_virtual(property)
                        && !has_hook(property, "set")
                {
                    self.inherit_property_hooks(property, requirement, unit)?;
                } else {
                    self.check_property_hook_signatures(property, requirement, unit)?;
                    self.check_property_type(property, requirement, unit)?;
                }
                self.check_plain_set_contract(property, requirement, "interface", unit)?;
            }
        }
        let mut missing = Vec::new();
        for property in defaults {
            let Some(linked) = &property.hooks else {
                continue;
            };
            if linked.hooks.is_virtual && property.initialized {
                return Err(NativeError::error(format!(
                    "Cannot specify default value for virtual hooked property {}::${}",
                    property.declaring, property.name
                )));
            }
            self.verify_linked_property_hooks(property, linked, &declaration.name)?;
            if declaration.is_abstract
                || matches!(declaration.kind, ClassKind::Interface | ClassKind::Trait)
            {
                continue;
            }
            for hook in linked.methods().filter(|hook| hook.is_abstract) {
                missing.push(self.code.function_name(hook.function));
            }
        }
        if !missing.is_empty() {
            let name = super::trait_methods::display(&declaration.name);
            let count = missing.len();
            let plural = if count == 1 { "" } else { "s" };
            missing.truncate(3);
            if count > 3 {
                missing.push("...");
            }
            return Err(NativeError::error(
                if declaration.is_final || declaration.kind == ClassKind::Anonymous {
                    format!(
                        "Class {name} must implement {count} abstract method{plural} ({})",
                        missing.join(", ")
                    )
                } else {
                    format!(
                        "Class {name} contains {count} abstract method{plural} and must therefore be declared abstract or implement the remaining method{plural} ({})",
                        missing.join(", ")
                    )
                },
            ));
        }
        Ok(())
    }
}

pub(super) fn is_virtual(property: &PropertyDefault) -> bool {
    property
        .hooks
        .as_ref()
        .is_some_and(|hooks| hooks.hooks.is_virtual)
}

fn has_hook(property: &PropertyDefault, operation: &str) -> bool {
    property
        .hooks
        .as_ref()
        .is_some_and(|hooks| match operation {
            "get" => hooks.hooks.get.is_some(),
            _ => hooks.hooks.set.is_some(),
        })
}

pub(super) fn property_type(property: &PropertyDefault) -> Option<&str> {
    property.ty.as_ref().map(|ty| ty.display.as_str())
}

pub(super) fn hook_declaration_error(
    vm: &Vm,
    function: FunctionIndex,
    message: String,
) -> NativeError {
    NativeError::Unwind(phera_extension_api::Unwind(Box::new(
        crate::error_traces::DeclarationFailure {
            message,
            file: vm.code.unit(function).file.clone(),
            line: vm.code.function(function).definition_line as usize,
        },
    )))
}
