//! PHP constants instruction handlers, separate from dispatch.

use std::sync::Arc;

use phera_bytecode::{CompiledUnit, ConstId, NameId, Reg, SymbolId};
use phera_extension_api::NativeError;
use phera_value::Value;

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
        let value = self.fetch_global_constant(unit, constant)?;
        self.set(destination, value);
        Ok(())
    }

    pub(crate) fn fetch_global_constant(
        &mut self,
        unit: Arc<CompiledUnit>,
        symbol: SymbolId,
    ) -> Result<Value, NativeError> {
        let constant = self.constants.fetch(unit, symbol)?;
        Ok(self.report_global_constant(constant))
    }

    pub(crate) fn report_global_constant(
        &mut self,
        constant: crate::constant_table::GlobalConstant,
    ) -> Value {
        if let Some(message) = constant.deprecated {
            self.diagnostic("Deprecated", message.to_string());
        }
        constant.value
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
            let origin = self.current().map(|frame| frame.frame.function);
            let deprecated = self.current().and_then(|frame| {
                let metadata = frame.unit().constant_metadata.get(&name)?;
                let attr =
                    crate::attribute_diagnostics::attribute(&metadata.attributes, "Deprecated")?;
                Some(crate::attribute_diagnostics::deprecation_message(
                    attr,
                    &format!("Constant {name}"),
                ))
            });
            self.constants.insert(
                name,
                self.get(source),
                origin.map(crate::constant_table::ConstantOrigin::Declaration),
                deprecated,
            );
        }
        Ok(())
    }
}
