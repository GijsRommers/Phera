//! Allocation prepares constructors before sends and shallow clones preserve nested handles.

use phera_bytecode::{ClassKind, Reg, SymbolId};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;

impl Vm {
    /// Resolve a compiler symbol without namespace fallback, as PHP class names require.
    pub(crate) fn class_symbol(&self, symbol: SymbolId) -> Result<String, NativeError> {
        let name = self
            .current()
            .and_then(|frame| frame.unit.symbols.get(symbol.index()))
            .map(|symbol| symbol.written.clone())
            .ok_or_else(|| internal_error!("class symbol"))?;
        let context = self.current().and_then(|frame| frame.method.as_ref());
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
                .and_then(|context| context.scope.parent.as_ref())
                .map(|parent| parent.name.clone())
                .ok_or_else(|| {
                    NativeError::error(
                        "Cannot access \"parent\" when current class scope has no parent",
                    )
                }),
            _ => Ok(name),
        }
    }

    /// Populate defaults root-first before resolving the inherited constructor.
    pub(crate) fn new_object(&mut self, symbol: SymbolId) -> Result<(), NativeError> {
        let name = self.class_symbol(symbol)?;
        self.new_named_object(&name)
    }

    /// Computed and literal allocation share defaults, visibility and abstract checks.
    pub(crate) fn new_named_object(&mut self, name: &str) -> Result<(), NativeError> {
        let class = self
            .classes
            .get(name)
            .ok_or_else(|| NativeError::error(format!("Class \"{name}\" not found")))?;
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
            .and_then(|frame| frame.method.as_ref())
            .and_then(|context| context.receiver.clone())
            .ok_or_else(|| NativeError::error("Using $this when not in object context"))?;
        self.set(destination, Value::Object(receiver));
        Ok(())
    }
}
