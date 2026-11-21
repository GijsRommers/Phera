//! `MakeCallable`, `ReceiveDefault`, `DeclareFunction` and the start of a
//! method site's written-out call.

use phera_bytecode::{FunctionId, Reg, Target};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;

impl Vm {
    /// `CallMethod` that the loop handed over: run the `InitMethodCall` that
    /// follows it and go on after that, with the sends.
    pub(crate) fn start_written_out_call<const TRACK_GENERATORS: bool>(
        &mut self,
    ) -> Result<(), NativeError> {
        let frame = self.current().ok_or_else(|| internal_error!("no frame"))?;
        let ip = frame.ip();
        let Some(&phera_bytecode::Instruction::InitMethodCall { receiver, method }) =
            frame.function().instructions.get(ip)
        else {
            return Err(internal_error!("a method site without its InitMethodCall"));
        };
        self.jump(Target(ip as u32 + 1))?;
        if !(TRACK_GENERATORS && self.prepare_ready_native_method(receiver, method)) {
            self.init_method_call::<TRACK_GENERATORS>(receiver, method)?;
        }
        Ok(())
    }

    /// Implements PHP MakeCallable semantics outside the dispatch loop.
    pub(crate) fn make_callable(&mut self, destination: Reg) -> Result<(), NativeError> {
        let callee = self.take_prepared_callee()?;
        let object = self.closure_from_callee(callee)?;
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
            return Err(self.fatal_error(1, message, location));
        }
        self.function_table.insert(name, index);
        Ok(())
    }
}
