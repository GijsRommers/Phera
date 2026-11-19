//! Initializing `$ghost->name` enters its initializer on the PHP frame stack.
//! Reading `$ghost->name` inside the initializer uses the ordinary object.
//! A failing ghost restores its property table when the initializer frame unwinds.

use std::{collections::HashSet, rc::Rc};

use phera_extension_api::reflection::lazy::{
    MakeLazy, SKIP_DESTRUCTOR, SKIP_INITIALIZATION_ON_SERIALIZE,
};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, LazyObject, LazyStrategy, Object, PhpArray, Value};

use crate::Vm;
use crate::calls::call_execution::CallEntry;
use crate::calls::callback_binding::ArgumentOrigin;
use crate::classes::{Class, class::ClassOrigin};
use crate::property_hook_continuation::HookContinuation;

pub(crate) struct Initialization {
    object: Rc<Object>,
    pending: Option<Box<LazyObject>>,
    snapshot: Option<PhpArray>,
    strategy: LazyStrategy,
}

impl Initialization {
    pub(crate) fn finish(mut self, result: Value) -> NativeResult {
        match self.strategy {
            LazyStrategy::Ghost => {
                if !result.is_null() {
                    return Err(NativeError::type_error(
                        "Lazy object initializer must return NULL or no value",
                    ));
                }
                self.object.replace_lazy(None);
                self.pending = None;
                Ok(Value::Object(self.object.clone()))
            }
            LazyStrategy::Proxy => {
                let Value::Object(instance) = &*result.deref() else {
                    return Err(NativeError::type_error(format!(
                        "Lazy proxy factory must return an instance of a class compatible with {}, {} returned",
                        self.object.class.name(),
                        result.type_name()
                    )));
                };
                if !proxy_compatible(&self.object, instance) {
                    return Err(NativeError::type_error(format!(
                        "The real instance class {} is not compatible with the proxy class {}. The proxy must be a instance of the same class as the real instance, or a sub-class with no additional properties, and no overrides of the __destructor or __clone methods.",
                        instance.class.name(),
                        self.object.class.name(),
                    )));
                }
                if Rc::ptr_eq(&self.object, instance) || instance.lazy().is_some() {
                    return Err(NativeError::error(
                        "Lazy proxy factory must return a non-lazy object",
                    ));
                }
                self.object.update(|storage| *storage = PhpArray::new());
                self.object
                    .replace_lazy(Some(Box::new(LazyObject::Proxy(instance.clone()))));
                self.pending = None;
                Ok(Value::Object(instance.clone()))
            }
        }
    }
}

impl Drop for Initialization {
    fn drop(&mut self) {
        if let Some(pending) = self.pending.take() {
            if let Some(snapshot) = self.snapshot.take() {
                self.object.update(|storage| *storage = snapshot);
            }
            self.object.replace_lazy(Some(pending));
        }
    }
}

fn proxy_compatible(proxy: &Object, instance: &Object) -> bool {
    if Rc::ptr_eq(&proxy.class, &instance.class) {
        return true;
    }
    let (Some(proxy), Some(real)) = (
        proxy.class.as_any().downcast_ref::<Class>(),
        instance.class.as_any().downcast_ref::<Class>(),
    ) else {
        return false;
    };
    proxy.is_a(&real.name)
        && proxy.defaults.len() + proxy.static_properties.len()
            == real.defaults.len() + real.static_properties.len()
        && ["__destruct", "__clone"].into_iter().all(|name| {
            match (proxy.methods.get(name), real.methods.get(name)) {
                (Some(left), Some(right)) => Rc::ptr_eq(left, right),
                (None, None) => true,
                _ => false,
            }
        })
}

impl Vm {
    pub(crate) async fn make_object_lazy(
        &mut self,
        class: Rc<Class>,
        request: MakeLazy,
    ) -> NativeResult {
        let MakeLazy {
            object,
            initializer,
            strategy,
            method,
            options,
        } = request;
        let reset = object.is_some();
        let (number, parameter) = (
            if reset { 2 } else { 1 },
            if strategy == LazyStrategy::Ghost {
                "initializer"
            } else {
                "factory"
            },
        );
        let function = format!("ReflectionClass::{method}");
        self.autoload_callable(&initializer).await?;
        self.resolve_native_callback(&function, number, parameter, &initializer)?;
        let prefix = format!("{function}(): Argument #{} ($options)", number + 1);
        if options & !(SKIP_DESTRUCTOR | SKIP_INITIALIZATION_ON_SERIALIZE) != 0 {
            return Err(reflection_error(format!("{prefix} contains invalid flags")));
        }
        if !reset && options & SKIP_DESTRUCTOR != 0 {
            return Err(reflection_error(format!(
                "{prefix} does not accept ReflectionClass::SKIP_DESTRUCTOR"
            )));
        }
        if object
            .as_ref()
            .is_some_and(|object| object.is_uninitialized_lazy())
        {
            return Err(reflection_error("Object is already lazy"));
        }
        if matches!(
            object.as_ref().and_then(|object| object.lazy()).as_deref(),
            Some(LazyObject::Initializing)
        ) {
            return Err(NativeError::error(
                "Can not reset an object while it is being initialized",
            ));
        }
        check_lazy_class(&class)?;
        if !reset {
            Self::check_factory_class(&class)?;
        }
        let object = object.unwrap_or_else(|| self.classes.allocate(class.clone()));
        let properties: HashSet<_> = class
            .defaults
            .iter()
            .filter(|property| {
                !property
                    .hooks
                    .as_ref()
                    .is_some_and(|hooks| hooks.hooks.is_virtual)
            })
            .filter(|property| {
                !(property.is_readonly
                    && (class.is_final || property.is_final)
                    && object.has(&property.key))
            })
            .map(|property| property.key.clone())
            .collect();
        if reset {
            let actual = self
                .classes
                .for_object(&object)
                .ok_or_else(|| internal_error!("lazy reset class"))?;
            for (key, _) in object.to_array().iter() {
                if properties.contains(&key)
                    || !actual.defaults.iter().any(|property| property.key == key)
                {
                    crate::reference_holders::ReferenceHolders::untrack_object(&object, &key);
                    object.remove(&key);
                }
            }
        }
        object.replace_lazy((!properties.is_empty()).then(|| {
            Box::new(LazyObject::Pending {
                strategy,
                initializer,
                properties,
                skip_serialization: options & SKIP_INITIALIZATION_ON_SERIALIZE != 0,
            })
        }));
        Ok(if reset {
            Value::Null
        } else {
            Value::Object(object)
        })
    }

    pub(crate) fn restore_lazy_defaults(
        &mut self,
        object: &Object,
        keys: &HashSet<ArrayKey>,
    ) -> Result<(), NativeError> {
        let class = self
            .classes
            .for_object(object)
            .ok_or_else(|| internal_error!("lazy defaults class"))?;
        let old = object.to_array();
        let mut merged = PhpArray::new();
        for property in &class.defaults {
            if keys.contains(&property.key) {
                if let Some(value) = self.lazy_property_default(property)? {
                    merged.insert(property.key.clone(), value);
                }
            } else if let Some(value) = old.get(&property.key) {
                merged.insert(property.key.clone(), value.clone());
            }
        }
        for (key, value) in old.iter() {
            if !merged.contains_key(&key) && !keys.contains(&key) {
                merged.insert(key, value.clone());
            }
        }
        object.update(|storage| *storage = merged);
        Ok(())
    }

    pub(crate) fn lazy_property_default(
        &mut self,
        property: &crate::classes::class::PropertyDefault,
    ) -> Result<Option<Value>, NativeError> {
        if !property.initialized {
            return Ok(None);
        }
        let value = match &property.initializer {
            Some(initializer) => self.evaluate_property_initializer(initializer, property)?,
            None => property.value.clone(),
        };
        Ok(Some(value))
    }

    pub(crate) fn begin_lazy_initialization(
        &mut self,
        object: Rc<Object>,
    ) -> Result<(Initialization, Value), NativeError> {
        let pending = object.replace_lazy(Some(Box::new(LazyObject::Initializing)));
        let Some(LazyObject::Pending {
            strategy,
            initializer,
            ..
        }) = pending.as_deref()
        else {
            object.replace_lazy(pending);
            return Err(internal_error!("missing lazy initializer"));
        };
        let strategy = *strategy;
        let initializer = initializer.clone();
        let snapshot = (strategy == LazyStrategy::Ghost).then(|| object.to_array());
        let initialization = Initialization {
            object,
            pending,
            snapshot,
            strategy,
        };
        if let Some(LazyObject::Pending { properties, .. }) = initialization.pending.as_deref()
            && strategy == LazyStrategy::Ghost
        {
            self.restore_lazy_defaults(&initialization.object, properties)?;
        }
        Ok((initialization, initializer))
    }

    /// Ordinary property access suspends on a PHP frame just like a property hook.
    pub(crate) async fn suspend_lazy_initialization(
        &mut self,
        object: Rc<Object>,
        continuation: crate::lazy_continuation::LazyContinuation,
    ) -> Result<Option<crate::property_hook_continuation::ReturnedHook>, NativeError> {
        let (initialization, initializer) = self.begin_lazy_initialization(object.clone())?;
        let callee = self.resolve_callable(&initializer)?;
        let origin = callee.argument_binding.origin(ArgumentOrigin::Callback);
        self.prepare_call(callee)?;
        self.store_sent_value(0, Value::Object(object))?;
        let mut entry = self.with_argument_origin(origin, |vm| vm.enter_prepared_call(None))?;
        if matches!(entry, CallEntry::ConvertStrings) {
            self.coerce_call_strings(origin).await?;
            entry = self.with_argument_origin(origin, |vm| vm.enter_prepared_call(None))?;
        }
        let continuation = Box::new(HookContinuation::Lazy(initialization, continuation));
        if matches!(entry, CallEntry::Php) {
            self.call_stack.current_extras_mut().property_hook = Some(continuation);
            Ok(None)
        } else {
            let previous = std::mem::replace(&mut self.argument_origin, origin);
            let result = Box::pin(self.finish_native_window()).await;
            self.argument_origin = previous;
            Ok(Some(crate::property_hook_continuation::ReturnedHook {
                continuation,
                value: result?,
            }))
        }
    }
}

fn check_lazy_class(class: &Class) -> Result<(), NativeError> {
    if class.origin != ClassOrigin::User && class.name != "stdClass" {
        return Err(NativeError::error(format!(
            "Cannot make instance of internal class lazy: {} is internal",
            class.name
        )));
    }
    let mut ancestor = class.parent.as_deref();
    while let Some(parent) = ancestor {
        if parent.origin != ClassOrigin::User && parent.name != "stdClass" {
            return Err(NativeError::error(format!(
                "Cannot make instance of internal class lazy: {} inherits internal class {}",
                class.name, parent.name,
            )));
        }
        ancestor = parent.parent.as_deref();
    }
    Ok(())
}

fn reflection_error(message: impl Into<String>) -> NativeError {
    NativeError::throw("ReflectionException", message)
}

pub(crate) fn resolved_lazy_instance(mut object: Rc<Object>) -> Rc<Object> {
    while let Some(instance) = object.lazy_instance() {
        object = instance;
    }
    object
}

#[derive(Clone, Copy)]
pub(crate) enum LazyAccess {
    Read,
    Isset,
    Write,
    Unset,
    Reference,
}

impl Vm {
    /// A hooked or overloaded access can return without touching backing storage.
    pub(crate) fn property_requires_initialization(
        &self,
        object: &Object,
        name: &ArrayKey,
        access: LazyAccess,
    ) -> bool {
        use crate::magic_properties::PropertyHook;
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
        } else if !matches!(access, LazyAccess::Isset)
            && class.as_ref().is_none_or(|class| class.magic.is_none())
        {
            return false;
        }
        let magic = match access {
            LazyAccess::Read | LazyAccess::Reference => PropertyHook::Get,
            LazyAccess::Isset => PropertyHook::Isset,
            LazyAccess::Write => PropertyHook::Set,
            LazyAccess::Unset => PropertyHook::Unset,
        };
        class
            .as_ref()
            .is_none_or(|class| self.property_hook(class, object, name, magic).is_none())
    }
}
