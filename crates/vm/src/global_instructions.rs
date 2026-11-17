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
        let name = self.name(name)?;
        self.bind_global(local, name)?;
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
            let value = state.statics.borrow().get(&variable.0).cloned();
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
            state.statics.borrow_mut().insert(variable.0, value.clone());
            self.set(local, value);
            return Ok(());
        }
        let key = self.static_key(variable)?;
        self.static_variables.insert(key, value.clone());
        self.set(local, value);
        Ok(())
    }
}
