//! PHP global instructions instruction handlers, separate from dispatch.

use std::{cell::RefCell, rc::Rc};

use phera_bytecode::{NameId, Reg, StaticId, Target};
use phera_extension_api::NativeError;
use phera_value::{Value, to_php_string};

use crate::Vm;

impl Vm {
    /// Implements PHP BindDynamicGlobal semantics outside the dispatch loop.
    pub(crate) fn bind_dynamic_global(&mut self, name: Reg) -> Result<(), NativeError> {
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let name = to_php_string(&diagnostics.vm.operand(name), &mut diagnostics.buffer)?
            .to_string_lossy();
        drop(diagnostics);
        let local = self.named_slot(&name)?;
        self.bind_global(local, name)?;
        Ok(())
    }

    /// Implements PHP BindGlobal semantics outside the dispatch loop.
    pub(crate) fn bind_named_global(
        &mut self,
        local: Reg,
        name: NameId,
    ) -> Result<(), NativeError> {
        use crate::unit_caches::GlobalBinding;

        /// Where a global variable lives before `global $name` binds it.
        enum Place {
            /// A declared variable of the file-scope code.
            Register(GlobalBinding),
            /// A variable that code made by name (`$$name`, `extract`).
            ByName,
            /// The request's globals table.
            Table,
        }

        let index = name.index();
        let name = self.name(name)?;
        let place = match self.global_frame() {
            Some(frame) => match frame.declared_register(&name) {
                Some(register) => Place::Register(GlobalBinding::Register {
                    function: frame.frame.function,
                    register,
                }),
                None if frame.named_register(&name).is_some() => Place::ByName,
                None => Place::Table,
            },
            None => Place::Table,
        };
        // The interpreter loop binds the variable itself from now on
        // (`Machine::bind_known_global`), while it stays where it is.
        let binding = match place {
            Place::Register(binding) => {
                self.bind_global(local, name)?;
                Some(binding)
            }
            Place::ByName => {
                self.bind_global(local, name)?;
                None
            }
            Place::Table => {
                let value = self.globals.reference(name.clone());
                self.set(local, value);
                let index = self.globals.index_of(&name);
                index
                    .and_then(|index| u32::try_from(index).ok())
                    .map(|index| GlobalBinding::Table { index })
            }
        };
        if let (Some(binding), Some(frame)) = (binding, self.current()) {
            self.code
                .cache(frame.frame.function)
                .remember_global(index, binding);
        }
        Ok(())
    }

    /// Implements PHP BindStatic semantics outside the dispatch loop.
    pub(crate) fn bind_static(
        &mut self,
        local: Reg,
        variable: StaticId,
        initialized: Target,
    ) -> Result<(), NativeError> {
        if let Some(object) = self.lexical_closure() {
            let native = object.native.borrow();
            let state = native
                .as_ref()
                .and_then(|state| state.downcast_ref::<crate::closures::ClosureState>())
                .ok_or_else(|| internal_error!("closure static"))?;
            let value = state
                .statics
                .as_ref()
                .and_then(|statics| statics.borrow().get(&variable.0).cloned());
            if let Some(value) = value {
                self.set(local, value);
                self.jump(initialized)?;
            }
            return Ok(());
        }
        let key = self.static_key(variable)?;
        if let Some(value) = self.static_variables.get(&key).cloned() {
            self.set(local, value);
            self.jump(initialized)?;
        }
        Ok(())
    }

    /// Implements PHP InitStatic semantics outside the dispatch loop.
    pub(crate) fn init_static(
        &mut self,
        local: Reg,
        variable: StaticId,
        source: Reg,
    ) -> Result<(), NativeError> {
        let value = Value::Reference(Rc::new(RefCell::new(self.get(source))));
        if let Some(object) = self.lexical_closure() {
            let native = object.native.borrow();
            let state = native
                .as_ref()
                .and_then(|state| state.downcast_ref::<crate::closures::ClosureState>())
                .ok_or_else(|| internal_error!("closure static"))?;
            let statics = state
                .statics
                .as_ref()
                .ok_or_else(|| internal_error!("closure statics"))?;
            statics.borrow_mut().insert(variable.0, value.clone());
            self.set(local, value);
            return Ok(());
        }
        let key = self.static_key(variable)?;
        self.static_variables.insert(key, value.clone());
        self.set(local, value);
        Ok(())
    }
}
