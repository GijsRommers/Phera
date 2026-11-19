//! Initializing `$ghost->name` enters its initializer on the PHP frame stack.
//! Reading `$ghost->name` inside the initializer uses the ordinary object.
//! A failing ghost restores its property table when the initializer frame unwinds.

use std::{cell::RefCell, collections::HashSet, rc::Rc};

use phera_extension_api::reflection::lazy::{
    MakeLazy, SKIP_DESTRUCTOR, SKIP_INITIALIZATION_ON_SERIALIZE,
};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{LazyObject, LazyStrategy, Object, PhpArray, Value};

use crate::Vm;
use crate::calls::callback_binding::ArgumentOrigin;
use crate::calls::{call_execution::CallEntry, call_state::Callee};
use crate::classes::{Class, class::ClassOrigin};
use crate::property_hook_continuation::HookContinuation;

pub(crate) struct Initialization {
    object: Rc<Object>,
    pending: Option<Box<LazyObject>>,
    snapshot: Option<PhpArray>,
    strategy: LazyStrategy,
    references: Rc<RefCell<crate::reference_holders::ReferenceHolders>>,
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
                self.object.update(|storage| {
                    *storage = self
                        .object
                        .class
                        .order_lazy_properties(std::mem::take(storage));
                });
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
                clear_reference_holders(&self.object);
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
                clear_reference_holders(&self.object);
                for (key, value) in snapshot.iter() {
                    if let Value::Reference(reference) = value {
                        let _ = crate::reference_holders::ReferenceHolders::track_object(
                            &self.references,
                            &self.object,
                            &key,
                            reference,
                        );
                    }
                }
                self.object.update(|storage| *storage = snapshot);
            }
            self.object.replace_lazy(Some(pending));
        }
    }
}

fn clear_reference_holders(object: &Object) {
    object.with_properties(|properties| {
        for (key, value) in properties.iter() {
            if value.is_reference() {
                crate::reference_holders::ReferenceHolders::untrack_object(object, &key);
            }
        }
    });
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
        let prepared = self.resolve_native_callback(&function, number, parameter, &initializer)?;
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
        self.classes.has_property_views = true;
        self.classes.has_lazy_objects = true;
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
                prepared: Box::new(prepared),
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

    pub(crate) fn begin_lazy_initialization(
        &mut self,
        object: Rc<Object>,
    ) -> Result<(Initialization, Callee), NativeError> {
        let pending = object.replace_lazy(Some(Box::new(LazyObject::Initializing)));
        let Some(LazyObject::Pending {
            strategy, prepared, ..
        }) = pending.as_deref()
        else {
            object.replace_lazy(pending);
            return Err(internal_error!("missing lazy initializer"));
        };
        let strategy = *strategy;
        let prepared = prepared.downcast_ref::<Callee>().cloned();
        let Some(prepared) = prepared else {
            object.replace_lazy(pending);
            return Err(internal_error!("invalid lazy callable"));
        };
        let snapshot = (strategy == LazyStrategy::Ghost).then(|| object.to_array());
        let initialization = Initialization {
            object,
            pending,
            snapshot,
            strategy,
            references: self.property_references.clone(),
        };
        if let Some(LazyObject::Pending { properties, .. }) = initialization.pending.as_deref()
            && strategy == LazyStrategy::Ghost
        {
            self.restore_lazy_defaults(&initialization.object, properties)?;
        }
        Ok((initialization, prepared))
    }

    /// Native callbacks without a destination use the VM's callback boundary.
    pub(crate) async fn initialize_lazy_via_callback(
        &mut self,
        mut object: Rc<Object>,
    ) -> Result<Rc<Object>, NativeError> {
        loop {
            if let Some(instance) = object.lazy_instance() {
                object = instance;
            } else if object.is_uninitialized_lazy() {
                let (initialization, callee) = self.begin_lazy_initialization(object.clone())?;
                let value = Box::pin(
                    self.invoke_resolved_callable(callee, [Value::Object(object.clone())]),
                )
                .await?;
                let Value::Object(instance) = initialization.finish(value)? else {
                    return Err(internal_error!("lazy initialization result"));
                };
                object = instance;
            } else {
                return Ok(object);
            }
        }
    }

    /// Ordinary property access suspends on a PHP frame just like a property hook.
    pub(crate) async fn suspend_lazy_initialization(
        &mut self,
        object: Rc<Object>,
        continuation: crate::lazy_continuation::LazyContinuation,
    ) -> Result<(), NativeError> {
        let (initialization, callee) = self.begin_lazy_initialization(object.clone())?;
        let origin = callee.argument_binding.origin(ArgumentOrigin::Callback);
        self.prepare_call(callee)?;
        self.store_sent_value(0, Value::Object(object))?;
        self.warn_callback_references();
        if !self.error_handlers.pending.is_empty() {
            self.drain_error_handlers().await?;
        }
        let mut entry = self.with_argument_origin(origin, |vm| vm.enter_prepared_call(None))?;
        if matches!(entry, CallEntry::ConvertStrings) {
            self.coerce_call_strings(origin).await?;
            entry = self.with_argument_origin(origin, |vm| vm.enter_prepared_call(None))?;
        }
        let continuation = Box::new(HookContinuation::Lazy(initialization, continuation));
        if matches!(entry, CallEntry::Php) {
            self.call_stack.current_extras_mut().property_hook = Some(continuation);
        } else {
            let previous = std::mem::replace(&mut self.argument_origin, origin);
            let result = Box::pin(self.finish_native_window()).await;
            self.argument_origin = previous;
            if self.returned_hook.is_some() {
                return Err(internal_error!("overlapping lazy continuation"));
            }
            self.returned_hook = Some(crate::property_hook_continuation::ReturnedHook {
                continuation,
                value: result?,
            });
            self.pending_callbacks = true;
        }
        Ok(())
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
