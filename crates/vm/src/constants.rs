//! PHP constants instruction handlers, separate from dispatch.

use phera_bytecode::{ConstId, NameId, Reg, SymbolId};
use phera_extension_api::NativeError;

use crate::Vm;

impl Vm {
    /// Implements PHP LoadConst semantics outside the dispatch loop.
    pub(crate) fn load_constant(
        &mut self,
        destination: Reg,
        constant: ConstId,
    ) -> Result<(), NativeError> {
        let value = self
            .current()
            .and_then(|frame| frame.cache().constants.get(constant.index()))
            .cloned()
            .ok_or_else(|| internal_error!("constant"))?;
        self.set(destination, value);
        Ok(())
    }

    /// Implements PHP FetchConstant semantics outside the dispatch loop.
    pub(crate) fn fetch_constant(
        &mut self,
        destination: Reg,
        constant: SymbolId,
    ) -> Result<(), NativeError> {
        let unit = self
            .current()
            .ok_or_else(|| internal_error!("constant frame"))?
            .unit()
            .clone();
        let value = self.constants.fetch(unit, constant)?;
        self.set(destination, value);
        Ok(())
    }

    /// Implements PHP DeclareConstant semantics outside the dispatch loop.
    pub(crate) fn declare_constant(
        &mut self,
        name: NameId,
        source: Reg,
    ) -> Result<(), NativeError> {
        let name = self.name(name)?;
        if self.constants.contains_key(&name) {
            self.warn(format!("Constant {name} already defined"));
        } else {
            self.constants.insert(name, self.get(source));
        }
        Ok(())
    }
}
