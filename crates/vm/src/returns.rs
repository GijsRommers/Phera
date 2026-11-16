//! Return instructions preserve PHP aliases and stop at native callback boundaries.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::{Vm, dispatch::DispatchResult};

impl Vm {
    /// Reference returns carry the reference cell rather than its current value.
    pub(crate) fn return_reference(
        &mut self,
        source: Reg,
        stop: usize,
    ) -> Result<DispatchResult, NativeError> {
        let value = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(source.index()))
            .and_then(Option::as_ref)
            .cloned()
            .unwrap_or_default();
        if !value.is_reference() {
            self.diagnostic(
                "Notice",
                "Only variable references should be returned by reference".into(),
            );
        }
        self.return_value(value, stop)
            .map(DispatchResult::from_return)
    }

    /// Ordinary returns dereference the source before releasing its frame.
    pub(crate) fn return_register(
        &mut self,
        source: Reg,
        stop: usize,
    ) -> Result<DispatchResult, NativeError> {
        let value = self.read_local(source);
        self.return_value(value, stop)
            .map(DispatchResult::from_return)
    }

    pub(crate) fn return_null(&mut self, stop: usize) -> Result<DispatchResult, NativeError> {
        self.return_value(Value::Null, stop)
            .map(DispatchResult::from_return)
    }

    /// Calls contribute to cooperative scheduling even without backward jumps.
    pub(crate) async fn execute_call(
        &mut self,
        destination: Reg,
        work: &mut usize,
    ) -> Result<(), NativeError> {
        *work += 1;
        self.finish_call(destination).await
    }
}
