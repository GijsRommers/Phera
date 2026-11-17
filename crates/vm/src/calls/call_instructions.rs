//! PHP call instructions instruction handlers, separate from dispatch.

use phera_bytecode::{FunctionId, Instruction, Reg, Target};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;
use crate::calls::execution_loop::BatchExit;
use crate::fast::ScalarStep;

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
        if let Some(previous) = self.function_table.get(&name) {
            let previous_line = previous.unit.functions[previous.index].definition_line;
            let location = self.location();
            let message = format!(
                "Cannot redeclare function {name}() (previously declared in {}:{previous_line})",
                previous.unit.file
            );
            return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                crate::error_traces::DeclarationFailure {
                    message,
                    file: location.file,
                    line: location.line,
                },
            ))));
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
    /// Call and Return switch heap activations without entering the async dispatcher.
    pub(crate) fn call_step(
        &mut self,
        instruction: Instruction,
        stop: usize,
        work: &mut usize,
    ) -> Result<Option<BatchExit>, NativeError> {
        match instruction {
            Instruction::Call { dst } => {
                *work += 1;
                if !self.enter_prepared_call(Some(dst))? {
                    return Ok(Some(BatchExit::NativeCall(dst)));
                }
            }
            Instruction::Return { src } => {
                let Some(value) = self.read_defined_local(src) else {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                };
                if let Some(value) = self.return_value(value, stop)? {
                    return Ok(Some(BatchExit::Returned(value)));
                }
            }
            Instruction::ReturnNull => {
                if let Some(value) = self.return_value(Value::Null, stop)? {
                    return Ok(Some(BatchExit::Returned(value)));
                }
            }
            _ => {
                if self.prepare_target_instruction(instruction)? == ScalarStep::Fallback
                    && self.send_instruction(instruction)? == ScalarStep::Fallback
                    && self.dispatch_instruction(instruction, work)? == ScalarStep::Fallback
                {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
            }
        }
        Ok(None)
    }
    /// Resolve the target before any argument expression can change the caller.
    #[inline]
    pub(crate) fn prepare_target_instruction(
        &mut self,
        instruction: Instruction,
    ) -> Result<ScalarStep, NativeError> {
        match instruction {
            Instruction::InitCall { function } => self.init_call(function)?,
            Instruction::InitDynamicCall { callee } => self.init_dynamic_call(callee)?,
            Instruction::InitMethodCall { receiver, method } => {
                self.init_method_call(receiver, method)?;
            }
            Instruction::InitStaticCall {
                class,
                method,
                binding,
            } => {
                self.prepare_known_static_call(class, method, binding)?;
            }
            Instruction::DynamicMethodCall { receiver, method } => {
                self.dynamic_method_call(receiver, method)?;
            }
            Instruction::StaticCall {
                class,
                method,
                binding,
            } => {
                self.static_style_call(class, method, binding)?;
            }
            Instruction::New { class } => self.new_object(class)?,
            Instruction::NewDynamic { class } => {
                let name = self.class_target(class)?;
                self.new_named_object(&name)?;
            }
            _ => return Ok(ScalarStep::Fallback),
        }
        Ok(ScalarStep::Complete)
    }
    /// All argument sources write through the same reserved window.
    #[inline]
    pub(crate) fn send_instruction(
        &mut self,
        instruction: Instruction,
    ) -> Result<ScalarStep, NativeError> {
        match instruction {
            Instruction::SendValue { src } => {
                self.send(crate::calls::sends::SentArgument::Value(src))?
            }
            Instruction::SendLocal { local } => {
                self.send(crate::calls::sends::SentArgument::Local(local))?;
            }
            Instruction::SendPath { path } => {
                self.send(crate::calls::sends::SentArgument::Path(self.path(path)?))?
            }
            Instruction::SendSpread { src } => self.spread(src)?,
            Instruction::NameArgument { name } => self.name_argument(name)?,
            Instruction::MakeCallable { dst } => self.make_callable(dst)?,
            Instruction::ReceiveDefault { param, passed } => self.receive_default(param, passed)?,
            _ => return Ok(ScalarStep::Fallback),
        }
        Ok(ScalarStep::Complete)
    }
}
