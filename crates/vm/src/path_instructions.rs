//! PHP path instructions instruction handlers, separate from dispatch.

use phera_bytecode::{BinaryOp, FetchMode, IncDec, PathId, Reg};
use phera_extension_api::NativeError;

use crate::{Vm, ops};

impl Vm {
    /// Append through aliases using the ordinary PHP write semantics.
    pub(crate) fn append_local(&mut self, local: Reg, source: Reg) -> Result<(), NativeError> {
        let value = self.read_local(source);
        self.write_place(&crate::AccessPath::local(local).with_append(), value, false)
    }

    /// Implements PHP Fetch semantics outside the dispatch loop.
    pub(crate) fn fetch_path(
        &mut self,
        dst: Reg,
        path: PathId,
        mode: FetchMode,
    ) -> Result<(), NativeError> {
        let place = self.path(path)?;
        let value = self.read_place(&place, mode != phera_bytecode::FetchMode::Read)?;
        self.set(dst, value);
        Ok(())
    }
    /// Implements PHP AssignOpPath semantics outside the dispatch loop.
    pub(crate) fn assign_op_path(
        &mut self,
        op: BinaryOp,
        dst: Reg,
        path: PathId,
        src: Reg,
    ) -> Result<(), NativeError> {
        let place = self.path(path)?;
        let left = self.read_place(&place, false)?;
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let value = ops::binary(op, &left, &self.operand(src), &mut diagnostics);
        self.diagnostics_flush(diagnostics);
        let value = value?;
        self.write_place(&place, value.clone(), false)?;
        self.set(dst, value);
        Ok(())
    }
    /// Implements PHP IncDecPath semantics outside the dispatch loop.
    pub(crate) fn inc_dec_path(
        &mut self,
        mode: IncDec,
        dst: Reg,
        path: PathId,
    ) -> Result<(), NativeError> {
        let place = self.path(path)?;
        let old = self.read_place(&place, false)?;
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let value = if matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement) {
            phera_value::increment(&old, &mut diagnostics)
        } else {
            phera_value::decrement(&old, &mut diagnostics)
        };
        self.diagnostics_flush(diagnostics);
        let value = value?;
        self.write_place(&place, value.clone(), false)?;
        self.set(
            dst,
            if matches!(mode, IncDec::PostIncrement | IncDec::PostDecrement) {
                old
            } else {
                value
            },
        );
        Ok(())
    }
    /// Implements PHP AssignPath semantics outside the dispatch loop.
    pub(crate) fn assign_path(&mut self, path: PathId, src: Reg) -> Result<(), NativeError> {
        let value = self.read_local(src);
        self.write_place(&self.path(path)?, value, false)?;
        Ok(())
    }
    /// Implements PHP MakeReference semantics outside the dispatch loop.
    pub(crate) fn make_reference(&mut self, dst: Reg, path: PathId) -> Result<(), NativeError> {
        let value = self.reference(&self.path(path)?)?;
        self.set(dst, value);
        Ok(())
    }
    /// Implements PHP BindReference semantics outside the dispatch loop.
    pub(crate) fn bind_reference(&mut self, path: PathId, src: Reg) -> Result<(), NativeError> {
        let value = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(src.index()))
            .and_then(Option::as_ref)
            .cloned()
            .unwrap_or_default();
        self.write_place(&self.path(path)?, value, true)?;
        Ok(())
    }
}
