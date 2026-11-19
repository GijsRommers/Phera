//! `public int $x { set(int|float $value) {} }` accepts both numbers.
//! When overriding a setter, the inherited accepted inputs are a contract too.

use std::cell::OnceCell;

use phera_bytecode::{CompiledUnit, PropertyHook, Visibility};
use phera_extension_api::NativeError;

use super::class::PropertyDefault;
use super::property_hook_inheritance::{hook_declaration_error, is_virtual, property_type};
use super::property_hooks::LinkedPropertyHooks;
use super::{Method, MethodBody};
use crate::{Vm, code_table::FunctionIndex};

impl Vm {
    pub(super) fn check_property_hook_signatures(
        &mut self,
        property: &PropertyDefault,
        inherited: &PropertyDefault,
        unit: &CompiledUnit,
    ) -> Result<(), NativeError> {
        let (Some(child), Some(parent)) = (&property.hooks, &inherited.hooks) else {
            return Ok(());
        };
        for (child, parent) in [
            (&child.hooks.get, &parent.hooks.get),
            (&child.hooks.set, &parent.hooks.set),
        ] {
            if let (Some(child), Some(parent)) = (child, parent) {
                let child_method = self.property_hook_method(property, child);
                let parent_method = self.property_hook_method(inherited, parent);
                if parent.is_final {
                    return Err(hook_declaration_error(
                        self,
                        child.function,
                        format!(
                            "Cannot override final property hook {}()",
                            self.code.function(parent.function).name,
                        ),
                    ));
                }
                self.check_method_contract(&parent_method, &child_method, unit)?;
            }
        }
        Ok(())
    }

    fn property_hook_method(
        &self,
        property: &PropertyDefault,
        hook: &PropertyHook<FunctionIndex>,
    ) -> Method {
        let function = self.code.function_name(hook.function);
        let (declaring, name) = function
            .split_once("::")
            .unwrap_or((&property.declaring, function));
        Method {
            signature: OnceCell::new(),
            name: name.into(),
            declaring: declaring.into(),
            visibility: property.visibility,
            is_static: false,
            is_final: hook.is_final,
            is_abstract: hook.is_abstract,
            body: MethodBody::User(hook.function),
        }
    }

    pub(super) fn check_plain_set_contract(
        &self,
        property: &PropertyDefault,
        inherited: &PropertyDefault,
        inherited_kind: &str,
        unit: &CompiledUnit,
    ) -> Result<(), NativeError> {
        if is_virtual(property)
            || property
                .hooks
                .as_ref()
                .is_some_and(|hooks| hooks.hooks.set.is_some())
        {
            return Ok(());
        }
        let Some(setter) = inherited
            .hooks
            .as_ref()
            .and_then(|hooks| hooks.hooks.set.as_ref())
        else {
            return Ok(());
        };
        let function = self.code.function(setter.function);
        let Some(ty) = function
            .parameters
            .first()
            .and_then(|param| self.code.unit(setter.function).types.get(param.ty?.index()))
        else {
            return Ok(());
        };
        if self.type_subset(
            Some(&ty.display),
            &inherited.declaring,
            property_type(property),
            &property.declaring,
            unit,
        ) {
            return Ok(());
        }
        Err(NativeError::error(format!(
            "Set type of {}::${} must be supertype of {} (as in {inherited_kind} {})",
            property.declaring,
            property.name,
            super::signatures::ordered_magic_union(&ty.display),
            inherited.declaring
        )))
    }

    pub(super) fn verify_linked_property_hooks(
        &self,
        property: &PropertyDefault,
        linked: &LinkedPropertyHooks,
        class: &str,
    ) -> Result<(), NativeError> {
        let hooks = &linked.hooks;
        if let Some(get) = &hooks.get
            && hooks.set.is_some()
            && !hooks.is_virtual
            && self.code.function(get.function).returns_reference
        {
            return Err(NativeError::error(format!(
                "Get hook of backed property {class}::{} with set hook may not return by reference",
                property.name,
            )));
        }
        if hooks.is_virtual && property.set_visibility.is_some() {
            let restriction = match (&hooks.get, &hooks.set) {
                (None, Some(_)) => Some("Write-only"),
                (Some(_), None) => Some("Read-only"),
                _ => None,
            };
            if let Some(restriction) = restriction {
                return Err(NativeError::error(format!(
                    "{restriction} virtual property {class}::${} must not specify asymmetric visibility",
                    property.name,
                )));
            }
        }
        Ok(())
    }

    pub(super) fn check_hook_overrides(
        &self,
        property: &PropertyDefault,
        parent: Option<&super::Class>,
        interfaces: &[std::rc::Rc<super::Class>],
    ) -> Result<(), NativeError> {
        let Some(linked) = &property.hooks else {
            return Ok(());
        };
        for (kind, hook) in [("get", &linked.hooks.get), ("set", &linked.hooks.set)] {
            let Some(hook) = hook else { continue };
            if self
                .code
                .facts(hook.function)
                .class
                .and_then(|index| self.classes.numbered(index.number()))
                .is_some_and(|owner| owner.name != property.declaring)
            {
                continue;
            }
            if !self
                .code
                .local(hook.function)
                .and_then(|index| self.code.unit(hook.function).function_metadata.get(index))
                .is_some_and(|metadata| {
                    crate::attribute_diagnostics::attribute(
                        &metadata.declaration.attributes,
                        "Override",
                    )
                    .is_some()
                })
            {
                continue;
            }
            let matching = parent
                .into_iter()
                .chain(interfaces.iter().map(|class| class.as_ref()))
                .flat_map(|class| &class.defaults)
                .any(|inherited| {
                    inherited.name == property.name
                        && inherited.visibility != Visibility::Private
                        && (!is_virtual(inherited) && (kind == "get" || !inherited.is_readonly)
                            || inherited.hooks.as_ref().is_some_and(|hooks| {
                                if kind == "get" {
                                    hooks.hooks.get.is_some()
                                } else {
                                    hooks.hooks.set.is_some()
                                }
                            }))
                });
            if !matching {
                return Err(hook_declaration_error(
                    self,
                    hook.function,
                    format!(
                        "{}() has #[\\Override] attribute, but no matching parent method exists",
                        self.code.function_name(hook.function)
                    ),
                ));
            }
        }
        Ok(())
    }
}
