//! `MakeCallable`, `ReceiveDefault` and `DeclareFunction`.

use phera_bytecode::{FunctionId, Reg, Target};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;

impl Vm {
    /// Implements PHP MakeCallable semantics outside the dispatch loop.
    pub(crate) fn make_callable(&mut self, destination: Reg) -> Result<(), NativeError> {
        let call = self.call_stack.take_pending()?;
        self.registers.truncate(call.window.base);
        let object = self.closure_from_callee(call.callee)?;
        self.set(destination, Value::Object(object));
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

    /// `function f() {}` reached inside a condition: declare it now.
    pub(crate) fn declare_function(&mut self, function: FunctionId) -> Result<(), NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("declaration"))?;
        let main = self.code.facts(frame.frame.function).unit_first;
        let index = self.code.declared(main, function.index());
        let name = self.code.function(index).name.to_ascii_lowercase();
        if let Some(previous) = self.function_table.get(&name) {
            let previous_line = self.code.function(previous).definition_line;
            let location = self.location();
            let message = format!(
                "Cannot redeclare function {name}() (previously declared in {}:{previous_line})",
                self.code.unit(previous).file
            );
            return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                crate::error_traces::DeclarationFailure {
                    message,
                    file: location.file,
                    line: location.line,
                },
            ))));
        }
        self.function_table.insert(name, index);
        Ok(())
    }
}
