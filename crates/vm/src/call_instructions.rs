//! PHP call instructions instruction handlers, separate from dispatch.

use phera_bytecode::{FunctionId, Reg, Target};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;

impl Vm {
    /// Implements PHP MakeCallable semantics outside the dispatch loop.
    pub(crate) fn make_callable(&mut self, destination: Reg) -> Result<(), NativeError> {
        let call = self
            .call_stack
            .current_mut()
            .and_then(|frame| frame.calls.pop())
            .ok_or_else(|| internal_error!("callable"))?;
        self.set(destination, Value::from(call.callee.name.clone()));
        Ok(())
    }

    /// Implements PHP ReceiveDefault semantics outside the dispatch loop.
    pub(crate) fn receive_default(
        &mut self,
        param: Reg,
        passed: Target,
    ) -> Result<(), NativeError> {
        if self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(param.index()))
            .is_some_and(Option::is_some)
        {
            self.jump(passed)?;
        }
        Ok(())
    }

    /// Implements PHP DeclareFunction semantics outside the dispatch loop.
    pub(crate) fn declare_function(&mut self, function: FunctionId) -> Result<(), NativeError> {
        let unit = self
            .current()
            .ok_or_else(|| internal_error!("declaration"))?
            .unit
            .clone();
        let name = unit
            .functions
            .get(function.index())
            .ok_or_else(|| internal_error!("function"))?
            .name
            .to_ascii_lowercase();
        if self.function_table.contains(&name) {
            return Err(NativeError::error(format!(
                "Cannot redeclare function {name}()"
            )));
        }
        self.function_table.insert(
            name,
            crate::function_table::FunctionRef {
                unit,
                index: function.index(),
            },
        );
        Ok(())
    }
}
