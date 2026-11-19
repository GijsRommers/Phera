//! Reflection exposes hook functions as methods without adding ordinary method entries.

use std::{cell::OnceCell, rc::Rc};

use phera_bytecode::PropertyHook;
use phera_extension_api::NativeError;
use phera_extension_api::reflection::{
    FunctionTarget, MemberInfo, PropertyHandle, PropertyHookKind, PropertySlot,
};

use crate::Vm;
use crate::classes::{Class, Method, MethodBody, class::PropertyDefault};
use crate::code_table::FunctionIndex;

use super::exception;
use super::properties::property_at;

pub(super) fn hook(
    property: &PropertyDefault,
    kind: PropertyHookKind,
) -> Option<&PropertyHook<FunctionIndex>> {
    let hooks = &property.hooks.as_ref()?.hooks;
    match kind {
        PropertyHookKind::Get => hooks.get.as_ref(),
        PropertyHookKind::Set => hooks.set.as_ref(),
    }
}

impl Vm {
    pub(super) fn reflection_property_hook(
        &self,
        member: &MemberInfo,
        kind: PropertyHookKind,
    ) -> Option<FunctionTarget> {
        let property = member.property?;
        let class = self.classes.numbered(property.class)?;
        hook(property_at(&class, property.slot)?, kind)?;
        Some(FunctionTarget::PropertyHook { property, kind })
    }

    pub(super) fn reflection_hook_method(
        &self,
        handle: PropertyHandle,
        kind: PropertyHookKind,
    ) -> Result<(Rc<Class>, Rc<Method>), NativeError> {
        let missing = || exception("Property hook does not exist");
        let class = self.classes.numbered(handle.class).ok_or_else(missing)?;
        let property = property_at(&class, handle.slot).ok_or_else(missing)?;
        let hook = hook(property, kind).ok_or_else(missing)?;
        let owner = self
            .code
            .facts(hook.function)
            .class
            .and_then(|class| self.classes.numbered(class.number()))
            .ok_or_else(missing)?;
        let visibility = owner
            .defaults
            .iter()
            .find(|declared| declared.name == property.name && declared.declaring == owner.name)
            .map_or(property.visibility, |declared| declared.visibility);
        let method = Rc::new(Method {
            signature: OnceCell::new(),
            name: format!("${}::{}", property.name, kind.name()),
            declaring: owner.name.clone(),
            visibility,
            is_static: false,
            is_final: hook.is_final,
            is_abstract: hook.is_abstract,
            body: MethodBody::User(hook.function),
        });
        Ok((class, method))
    }

    pub(super) fn reflection_hook_prototype(
        &self,
        handle: PropertyHandle,
        kind: PropertyHookKind,
    ) -> Option<FunctionTarget> {
        let (class, method) = self.reflection_hook_method(handle, kind).ok()?;
        let declaring = self.classes.get(&method.declaring)?;
        let name = &property_at(&class, handle.slot)?.name;
        let mut parent = declaring.parent.as_deref();
        let parents = std::iter::from_fn(|| {
            let class = parent?;
            parent = class.parent.as_deref();
            Some(class)
        });
        crate::class_listings::interfaces_in_php_order(&class)
            .into_iter()
            .chain(parents)
            .find_map(|candidate| {
                if candidate.name == declaring.name {
                    return None;
                }
                let slot = candidate.defaults.iter().position(|property| {
                    let Some(hook) = hook(property, kind) else {
                        return false;
                    };
                    property.name == *name
                        && property.visibility != phera_bytecode::Visibility::Private
                        && self.code.facts(hook.function).class != Some(declaring.index)
                })?;
                Some(FunctionTarget::PropertyHook {
                    property: PropertyHandle {
                        class: candidate.index.number(),
                        slot: PropertySlot::Instance(slot),
                    },
                    kind,
                })
            })
    }

    pub(super) fn reflection_property_settable_type(&self, member: &MemberInfo) -> Option<String> {
        let Some(property) = member.property else {
            return member.declared_type.clone();
        };
        let class = self.classes.numbered(property.class)?;
        let declaration = property_at(&class, property.slot)?;
        if let Some(setter) = hook(declaration, PropertyHookKind::Set) {
            let body = self.code.function(setter.function);
            let unit = self.code.unit(setter.function);
            return body.parameters.first().and_then(|parameter| {
                parameter
                    .ty
                    .map(|ty| unit.types[ty.index()].display.clone())
            });
        }
        if declaration
            .hooks
            .as_ref()
            .is_some_and(|hooks| hooks.hooks.is_virtual)
        {
            return Some("never".into());
        }
        member.declared_type.clone()
    }
}
