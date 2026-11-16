//! PHP global instructions instruction handlers, separate from dispatch.

use std::{cell::RefCell, rc::Rc};

use phera_bytecode::{NameId, Reg, StaticId, Target};
use phera_extension_api::NativeError;
use phera_value::{Value, to_php_string};

use crate::Vm;

impl Vm {
    /// Implements PHP BindDynamicGlobal semantics outside the dispatch loop.
    pub(crate) fn bind_dynamic_global(&mut self, name: Reg) -> Result<(), NativeError> {
        let mut diagnostics = self.diagnostic_reporter.take_operator();
        let name = to_php_string(&self.operand(name), &mut diagnostics)?.to_string_lossy();
        self.diagnostics_flush(diagnostics);
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
        let key = self.static_key(variable)?;
        let value = Value::Reference(Rc::new(RefCell::new(self.get(source))));
        self.static_variables.insert(key, value.clone());
        self.set(local, value);
        Ok(())
    }
}
