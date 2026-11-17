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
            .and_then(|frame| frame.unit(&self.code).symbols.get(symbol.index()))
            .map(|symbol| symbol.written.clone())
            .ok_or_else(|| internal_error!("class symbol"))?;
        let context = self
            .current()
            .and_then(|frame| self.call_stack.extras[frame.depth].method.as_ref());
        match name.as_str() {
            "self" | "static" => {
                let context = context.ok_or_else(|| {
                    NativeError::error(format!(
                        "Cannot access \"{name}\" when no class scope is active"
                    ))
                })?;
                Ok(if name == "self" {
                    self.classes.at(context.scope).name.clone()
                } else {
                    self.classes.at(context.called).name.clone()
                })
            }
            "parent" => context
                .and_then(|context| self.classes.at(context.scope).parent.as_ref())
                .map(|parent| parent.name.clone())
                .ok_or_else(|| {
                    NativeError::error(
                        "Cannot access \"parent\" when current class scope has no parent",
                    )
                }),
            _ => Ok(name),
        }
    }

    /// Class links are permanent; relative keywords select the current numeric scope.
    pub(crate) fn class_index(
        &mut self,
        symbol: SymbolId,
    ) -> Result<super::class_table::ClassIndex, NativeError> {
        use crate::code_table::ClassSymbol;
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("class frame"))?;
        let unit_index = self.code.functions[frame.function.0 as usize].unit;
        let unit = &self.code.units[unit_index];
        if let ClassSymbol::Named(name) = unit.class_symbols[symbol.index()] {
            if let Some(index) = unit.class_links.borrow()[symbol.index()] {
                return Ok(index);
            }
            let index = self.classes.named(name).ok_or_else(|| {
                NativeError::error(format!(
                    "Class \"{}\" not found",
                    unit.unit.symbols[symbol.index()].written
                ))
            })?;
            self.code.units[unit_index].class_links.borrow_mut()[symbol.index()] = Some(index);
            return Ok(index);
        }
        let context = self.call_stack.extras[frame.depth].method.as_ref();
        let resolved = match unit.class_symbols[symbol.index()] {
            ClassSymbol::Lexical => context.map(|context| context.scope),
            ClassSymbol::Called => context.map(|context| context.called),
            ClassSymbol::Parent => context
                .and_then(|context| self.classes.at(context.scope).parent.as_ref())
                .map(|parent| parent.index),
            ClassSymbol::Named(_) => None,
        };
        resolved.ok_or_else(|| {
            let name = &unit.unit.symbols[symbol.index()].written;
            if matches!(unit.class_symbols[symbol.index()], ClassSymbol::Parent) {
                NativeError::error(
                    "Cannot access \"parent\" when current class scope has no parent",
                )
            } else {
                NativeError::error(format!(
                    "Cannot access \"{name}\" when no class scope is active"
                ))
            }
        })
    }

    /// Constructors and ordinary method calls share permanent declaration metadata.
    pub(crate) fn new_object(&mut self, symbol: SymbolId) -> Result<(), NativeError> {
        let index = self.class_index(symbol)?;
        let class = self.classes.at(index).clone();
        self.new_linked_object(class)
    }

    /// Computed and literal allocation share defaults, visibility and abstract checks.
    pub(crate) fn new_named_object(&mut self, name: &str) -> Result<(), NativeError> {
        let class = self
            .classes
            .get(name)
            .ok_or_else(|| NativeError::error(format!("Class \"{name}\" not found")))?;
        self.new_linked_object(class)
    }

    /// Mirrors named allocation after lookup; visibility and abstract checks stay shared.
    fn new_linked_object(&mut self, class: Rc<super::Class>) -> Result<(), NativeError> {
        if class.origin == crate::classes::class::ClassOrigin::Closure {
            return Err(NativeError::error(
                "Instantiation of class Closure is not allowed",
            ));
        }
        if class.kind == ClassKind::Interface {
            return Err(NativeError::error(format!(
                "Cannot instantiate interface {}",
                class.name
            )));
        }
        if class.is_abstract {
            return Err(NativeError::error(format!(
                "Cannot instantiate abstract class {}",
                class.name
            )));
        }
        let object = self.classes.allocate(class.clone());
        // Mirrors the default loop below for classes whose defaults are all literals.
        if let Some(template) = &class.property_template {
            object.update(|properties| *properties = template.clone());
        } else {
            for property in &class.defaults {
                if property.initialized {
                    let value = if let Some(initializer) = &property.initializer {
                        self.evaluate_initializer(initializer, &property.declaring)?
                    } else {
                        property.value.clone()
                    };
                    object.set(property.key.clone(), value);
                }
            }
        }
        if class.is_throwable {
            self.initialize_throwable(&object, crate::exceptions::TraceOrigin::Construction);
        }
        if class.constructor.is_none() {
            return self.prepare_constructed_object(&class, object);
        }
        self.prepare_method(
            class,
            Some(object),
            "__construct",
            super::Invocation::Constructor,
        )
    }

    /// Clone bypasses constructors and copies only the property table, not nested objects.
    pub(crate) fn clone_object(
        &mut self,
        destination: Reg,
        source: Reg,
    ) -> Result<(), NativeError> {
        let Value::Object(original) = self.read_local(source) else {
            return Err(NativeError::error("__clone method called on non-object"));
        };
        let class = self
            .classes
            .get(original.class.name())
            .ok_or_else(|| internal_error!("clone class"))?;
        if class.is_throwable {
            return Err(NativeError::error(format!(
                "Trying to clone an uncloneable object of class {}",
                class.name
            )));
        }
        if class.origin == crate::classes::class::ClassOrigin::Closure {
            let native = original.native.borrow();
            let state = native
                .as_ref()
                .and_then(|state| state.downcast_ref::<crate::closures::ClosureState>())
                .ok_or_else(|| internal_error!("clone closure"))?;
            let cloned = self.allocate_closure(state.duplicate())?;
            self.set(destination, Value::Object(cloned));
            return Ok(());
        }
        let cloned = self.classes.allocate(class);
        cloned.update(|properties| *properties = original.to_array());
        self.set(destination, Value::Object(cloned));
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
                .is_some_and(|class| class.is_a(&name)),
            _ => false,
        };
        self.set(destination, Value::Bool(matches));
        Ok(())
    }

    /// $this is not a local variable and is unavailable in a static/global frame.
    pub(crate) fn fetch_this(&mut self, destination: Reg) -> Result<(), NativeError> {
        let receiver = self
            .current()
            .and_then(|frame| self.call_stack.extras[frame.depth].method.as_ref())
            .and_then(|context| self.frame_receiver(context).cloned())
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
            .unit(&self.code)
            .symbols
            .get(symbol.index())
            .ok_or_else(|| internal_error!("class name symbol"))?
            .written;
        let context = self.call_stack.extras[frame.depth].method.as_ref();
        let class = match name.as_str() {
            "self" => context.map(|context| self.classes.at(context.scope)),
            "static" => context.map(|context| self.classes.at(context.called)),
            "parent" => context.and_then(|context| self.classes.at(context.scope).parent.as_ref()),
            _ => {
                return frame
                    .cache(&self.code)
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
