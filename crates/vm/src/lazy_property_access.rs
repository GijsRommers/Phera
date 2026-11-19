//! A virtual getter can return without touching the ghost: `public int $n { get => 1; }`.

use std::rc::Rc;

use crate::{Vm, magic_properties::PropertyHook};
use phera_value::{ArrayKey, LazyObject, Object};

#[derive(Clone, Copy)]
pub(crate) enum LazyAccess {
    Read,
    Isset,
    Coalesce,
    Write,
    Unset,
    Reference,
}

impl LazyAccess {
    fn magic(self) -> PropertyHook {
        match self {
            Self::Read | Self::Reference => PropertyHook::Get,
            Self::Isset | Self::Coalesce => PropertyHook::Isset,
            Self::Write => PropertyHook::Set,
            Self::Unset => PropertyHook::Unset,
        }
    }
}

impl Vm {
    /// Property storage is forwarded to the backing object. The wrapper can
    /// still intercept a missing property with its own magic method.
    pub(crate) fn lazy_property_receiver(
        &self,
        original: &Rc<Object>,
        name: &ArrayKey,
        access: LazyAccess,
    ) -> Rc<Object> {
        let Some(instance) = original.lazy_instance() else {
            return original.clone();
        };
        let instance = crate::lazy_objects::resolved_lazy_instance(instance);
        let Some(class) = self.classes.for_object(original) else {
            return instance;
        };
        let hook = access.magic();
        let magic = self.property_hook(&class, original, name, hook).is_some()
            || matches!(access, LazyAccess::Coalesce)
                && self
                    .property_hook(&class, original, name, PropertyHook::Get)
                    .is_some();
        if magic && !self.property_guarded(&instance, name, hook) {
            let display = crate::path_update::property_name(name);
            let (value, key) = self.property_slot(&instance, &display);
            let declared =
                key.is_some_and(|key| class.defaults.iter().any(|property| property.key == key));
            if value.is_none() && !declared {
                return original.clone();
            }
        }
        instance
    }

    /// A hooked or overloaded access can return without touching backing storage.
    pub(crate) fn property_requires_initialization(
        &self,
        object: &Object,
        name: &ArrayKey,
        access: LazyAccess,
    ) -> bool {
        let Some(state) = object.lazy() else {
            return false;
        };
        let LazyObject::Pending { properties, .. } = &*state else {
            return false;
        };
        let display = crate::path_update::property_name(name);
        let class = self.classes.for_object(object);
        if let Ok(key) = self.property_key(object, &display) {
            if let Some(property) = class
                .as_ref()
                .and_then(|class| class.defaults.iter().find(|property| property.key == key))
            {
                if let Some(hooks) = &property.hooks {
                    let hook = match access {
                        LazyAccess::Write => hooks.hooks.set.as_ref(),
                        LazyAccess::Unset => return false,
                        _ => hooks.hooks.get.as_ref(),
                    };
                    if hooks.hooks.is_virtual
                        || hook.is_some() && !self.uses_property_backing(object, property, hooks)
                    {
                        return false;
                    }
                }
                return properties.contains(&key);
            }
            if object.has(&key) {
                return false;
            }
        } else if !matches!(access, LazyAccess::Isset | LazyAccess::Coalesce)
            && class.as_ref().is_none_or(|class| class.magic.is_none())
        {
            return false;
        }
        class.as_ref().is_none_or(|class| {
            self.property_hook(class, object, name, access.magic())
                .is_none()
                && (!matches!(access, LazyAccess::Coalesce)
                    || self
                        .property_hook(class, object, name, PropertyHook::Get)
                        .is_none())
        })
    }
}
