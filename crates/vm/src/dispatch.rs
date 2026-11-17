//! Cold instruction dispatch delegates PHP behavior to concern-specific handlers.

use phera_bytecode::Instruction;
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;

/// Returning can finish a native callback boundary or the main script.
pub(crate) enum DispatchResult {
    Continue,
    Returned(Value),
}

impl DispatchResult {
    pub(crate) fn from_return(value: Option<Value>) -> Self {
        value.map_or(Self::Continue, Self::Returned)
    }
}

impl Vm {
    /// Only operations that can suspend leave the synchronous interpreter loop.
    pub(crate) async fn dispatch(
        &mut self,
        instruction: Instruction,
        stop: usize,
        _work: &mut usize,
    ) -> Result<DispatchResult, NativeError> {
        match instruction {
            Instruction::ReturnChecked { source } => {
                return self
                    .dispatched_return(
                        crate::calls::returns::DispatchedReturn::Checked(source),
                        stop,
                    )
                    .await;
            }
            Instruction::ReturnReference { src: source } => {
                return self
                    .dispatched_return(
                        crate::calls::returns::DispatchedReturn::Reference(source),
                        stop,
                    )
                    .await;
            }
            Instruction::Return { src: source } => {
                return self
                    .dispatched_return(crate::calls::returns::DispatchedReturn::Value(source), stop)
                    .await;
            }

            Instruction::Exit { src: source } => {
                return self.exit_value(source).map(DispatchResult::Returned);
            }
            Instruction::Include {
                dst: destination,
                path,
                kind,
            } => self.include(destination, path, kind).await?,
            _ => return Err(internal_error!("unimplemented instruction")),
        }
        Ok(DispatchResult::Continue)
    }
}
