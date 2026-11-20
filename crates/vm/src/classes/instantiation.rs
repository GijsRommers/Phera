//! Allocation prepares constructors before sends and shallow clones preserve nested handles.

use phera_bytecode::{ClassKind, Reg, SymbolId};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;
use std::rc::Rc;

impl Vm {
    /// Resolve a compiler symbol without namespace fallback, as PHP class names require.
    pub(crate) fn class_symbol(&self, symbol: SymbolId) -> Result<String, NativeError> {
        let name = self
            .current()
            .and_then(|frame| frame.unit().symbols.get(symbol.index()))
            .map(|symbol| symbol.written.clone())
            .ok_or_else(|| internal_error!("class symbol"))?;
        let context = self.current().and_then(|frame| frame.method());
        match name.as_str() {
            "self" | "static" => {
                let context = context.ok_or_else(|| {
                    NativeError::error(format!(
                        "Cannot access \"{name}\" when no class scope is active"
                    ))
                })?;
                Ok(if name == "self" {
                    context.scope.name.clone()
                } else {
                    context.called.name.clone()
                })
            }
            "parent" => context
                .and_then(|context| context.scope.parent.clone())
                .map(|parent| parent.name.clone())
                .ok_or_else(|| {
                    NativeError::error(
                        "Cannot access \"parent\" when current class scope has no parent",
                    )
                }),
            _ => Ok(name),
        }
    }

    /// Remember which class a named class symbol of the running file means.
    /// Relative names (`self`, `parent`, `static`) depend on the frame and
    /// are never linked. A declared class cannot go away, so the link holds
    /// for the rest of the request.
    pub(crate) fn link_class_symbol(&self, symbol: SymbolId, class: &Rc<super::Class>) {
        let Some(frame) = self.current() else {
            return;
        };
        let relative = frame
            .unit()
            .symbols
            .get(symbol.index())
            .is_none_or(|symbol| matches!(symbol.written.as_str(), "self" | "parent" | "static"));
        if !relative && let Some(slot) = frame.cache().classes.get(symbol.index()) {
            *slot.borrow_mut() = Some(class.clone());
        }
    }

    /// Literal class cache is keyed by unit/symbol, filled after successful lookup.
    /// Relative keywords bypass it. Linked classes cannot be replaced in a request,
    /// and the unit/class owners keep identities alive until request teardown.
    /// Populate defaults root-first before resolving the inherited constructor.
    pub(crate) fn new_object(&mut self, symbol: SymbolId) -> Result<(), NativeError> {
        let frame = self.current().ok_or_else(|| internal_error!("new site"))?;
        let written = &frame
            .unit()
            .symbols
            .get(symbol.index())
            .ok_or_else(|| internal_error!("new symbol"))?
            .written;
        if matches!(written.as_str(), "self" | "parent" | "static") {
            let name = self.class_symbol(symbol)?;
            return self.new_named_object(&name);
        }
        let cached = frame
            .cache()
            .classes
            .get(symbol.index())
            .and_then(|slot| slot.borrow().clone());
        let class = if let Some(class) = cached {
            class
        } else {
            let class = self
                .classes
                .get(written)
                .ok_or_else(|| Self::class_miss(written, "Class"))?;
            if let Some(slot) = self
                .current()
                .and_then(|frame| frame.cache().classes.get(symbol.index()))
            {
                *slot.borrow_mut() = Some(class.clone());
            }
            class
        };
        self.new_linked_object(class)
    }

    /// Computed and literal allocation share defaults, visibility and abstract checks.
    pub(crate) fn new_named_object(&mut self, name: &str) -> Result<(), NativeError> {
        let class = self
            .classes
            .get(name)
            .ok_or_else(|| Self::class_miss(name, "Class"))?;
        self.new_linked_object(class)
    }

    /// Mirrors named allocation after lookup; visibility and abstract checks stay shared.
    fn new_linked_object(&mut self, class: Rc<super::Class>) -> Result<(), NativeError> {
        if class.origin == crate::classes::class::ClassOrigin::Closure {
            return Err(NativeError::error(
                "Instantiation of class Closure is not allowed",
            ));
        }
        if class.name == "Generator" {
            return Err(NativeError::error(
                crate::generators::class::INSTANTIATION_ERROR,
            ));
        }
        if matches!(class.kind, ClassKind::Interface | ClassKind::Trait) {
            let kind = if class.kind == ClassKind::Trait {
                "trait"
            } else {
                "interface"
            };
            return Err(NativeError::error(format!(
                "Cannot instantiate {kind} {}",
                class.name
            )));
        }
        if class.is_abstract {
            return Err(NativeError::error(format!(
                "Cannot instantiate abstract class {}",
                class.name
            )));
        }
        if class.kind == ClassKind::Enum {
            return Err(NativeError::error(format!(
                "Cannot instantiate enum {}",
                class.name
            )));
        }
        Self::check_script_construction(&class)?;
        let properties = if let Some(template) = &class.property_template {
            template.clone()
        } else {
            let mut properties = phera_value::PhpArray::new();
            for property in &class.defaults {
                if property.initialized {
                    let value = if let Some(initializer) = &property.initializer {
                        self.evaluate_property_initializer(initializer, property)?
                    } else {
                        property.value.clone()
                    };
                    properties.insert(property.key.clone(), value);
                }
            }
            properties
        };
        let object = self
            .classes
            .allocate_with_properties(class.clone(), properties);
        if class.is_throwable {
            self.initialize_throwable(&object, crate::exceptions::TraceOrigin::Construction);
        }
        let Some(constructor) = class.constructor.clone() else {
            self.prepare_constructed_object(object);
            return Ok(());
        };
        if constructor.visibility == phera_bytecode::Visibility::Public
            && let Some(scope) = super::method_lookup::declaring_class(&class, &constructor)
        {
            let resolved = super::method_lookup::ResolvedMethod {
                method: constructor,
                scope,
            };
            let invocation = super::Invocation::Constructor;
            return self.prepare_resolved_method(class, Some(object), resolved, invocation);
        }
        let prepared = self.prepare_method(
            class,
            Some(object.clone()),
            "__construct",
            super::Invocation::Constructor,
        );
        // A constructor that may not be called leaves an object that was
        // never constructed: PHP runs no destructor for it.
        if prepared.is_err() {
            object.mark_destructed();
        }
        prepared
    }

    pub(crate) fn check_script_construction(class: &super::Class) -> Result<(), NativeError> {
        if let Some(message) = class
            .hooks
            .as_ref()
            .and_then(|hooks| hooks.construction_error)
        {
            return Err(NativeError::error(message));
        }
        Ok(())
    }

    /// Unknown names simply fail membership tests rather than raising a missing-class error.
    pub(crate) fn instance_of(
        &mut self,
        destination: Reg,
        source: Reg,
        symbol: SymbolId,
    ) -> Result<(), NativeError> {
        let name = self.class_symbol(symbol)?;
        let matches = match self.read_local(source) {
            Value::Object(object) => self
                .classes
                .get(object.class.name())
                .is_some_and(|class| self.class_matches(&class, &name)),
            _ => false,
        };
        self.set(destination, Value::Bool(matches));
        Ok(())
    }

    /// $this is not a local variable and is unavailable in a static/global frame.
    pub(crate) fn fetch_this(&mut self, destination: Reg) -> Result<(), NativeError> {
        let receiver = self
            .current()
            .and_then(|frame| frame.this().cloned())
            .ok_or_else(|| NativeError::error("Using $this when not in object context"))?;
        self.set(destination, Value::Object(receiver));
        Ok(())
    }
}

impl Vm {
    /// Mirrors class_symbol's binding rules while sharing immutable PHP name strings.
    pub(crate) fn class_symbol_value(&self, symbol: SymbolId) -> Result<Value, NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("class name frame"))?;
        let name = &frame
            .unit()
            .symbols
            .get(symbol.index())
            .ok_or_else(|| internal_error!("class name symbol"))?
            .written;
        let class = match name.as_str() {
            phera_bytecode::TRAIT_CLASS_NAME => {
                let function = frame.frame.function;
                self.code
                    .facts(function)
                    .class
                    .map(|class| self.classes.class(class))
                    .or_else(|| frame.scope())
                    .cloned()
            }
            "self" => frame.scope().cloned(),
            "static" => frame.called(),
            "parent" => frame.scope().and_then(|scope| scope.parent.clone()),
            _ => {
                return frame
                    .cache()
                    .class_names
                    .get(symbol.index())
                    .cloned()
                    .ok_or_else(|| internal_error!("class name cache"));
            }
        };
        if let Some(class) = class {
            return Ok(class.name_value.clone());
        }
        // Preserve exact diagnostics for missing relative scopes in the resolver.
        self.class_symbol(symbol).map(Value::from)
    }
}
