//! Global and static reference identity.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;

use crate::Vm;

/// Trait imports have distinct function indices even when the source bytecode is shared.
#[derive(Hash, PartialEq, Eq)]
pub(crate) struct StaticKey {
    pub(crate) function: crate::code_table::FunctionIndex,
    pub(crate) variable: u16,
}

impl Vm {
    pub(crate) fn name(&self, name: phera_bytecode::NameId) -> Result<String, NativeError> {
        self.current()
            .and_then(|frame| frame.unit().names.get(name.index()))
            .cloned()
            .ok_or_else(|| internal_error!("invalid name"))
    }

    /// Distinguish static cells by unit allocation, function and declared static slot.
    pub(crate) fn static_key(
        &self,
        variable: phera_bytecode::StaticId,
    ) -> Result<StaticKey, NativeError> {
        let frame = self.current().ok_or_else(|| internal_error!("static"))?;
        Ok(StaticKey {
            function: frame.frame.function,
            variable: variable.0,
        })
    }

    /// A local declaration aliases the request symbol, wherever it currently lives.
    pub(crate) fn bind_global(&mut self, local: Reg, name: String) -> Result<(), NativeError> {
        let value = self.global_reference(name);
        self.set(local, value);
        Ok(())
    }
}
