//! PHP path instructions instruction handlers, separate from dispatch.

use phera_bytecode::{BinaryOp, FetchMode, IncDec, PathId, Reg};
use phera_extension_api::NativeError;

use crate::{Vm, ops};

impl Vm {
    /// Append through aliases using the ordinary PHP write semantics.
    pub(crate) fn append_local(&mut self, local: Reg, source: Reg) -> Result<(), NativeError> {
        let value = self.read_local(source);
        self.write_place(
            &crate::AccessPath::local(local).with_append(),
            value,
            crate::access_mode::Binding::ByValue,
        )
    }

    /// Implements PHP Fetch semantics outside the dispatch loop.
    pub(crate) fn fetch_path(
        &mut self,
        destination: Reg,
        path: PathId,
        mode: FetchMode,
    ) -> Result<(), NativeError> {
        let place = self.path(path)?;
        let value = self.read_place(
            &place,
            if mode == FetchMode::Read {
                crate::access_mode::Missing::Warn
            } else {
                crate::access_mode::Missing::Silent
            },
        )?;
        self.set(destination, value);
        Ok(())
    }

    /// Implements PHP AssignOpPath semantics outside the dispatch loop.
    pub(crate) fn assign_op_path(
        &mut self,
        operator: BinaryOp,
        destination: Reg,
        path: PathId,
        source: Reg,
    ) -> Result<(), NativeError> {
        let place = self.path(path)?;
        let left = self.read_place(&place, crate::access_mode::Missing::Warn)?;
        let mut diagnostics = self.diagnostic_reporter.take_operator();
        let value = ops::binary(operator, &left, &self.operand(source), &mut diagnostics);
        self.diagnostics_flush(diagnostics);
        let value = value?;
        self.write_place(&place, value.clone(), crate::access_mode::Binding::ByValue)?;
        self.set(destination, value);
        Ok(())
    }

    /// Implements PHP IncDecPath semantics outside the dispatch loop.
    pub(crate) fn inc_dec_path(
        &mut self,
        mode: IncDec,
        destination: Reg,
        path: PathId,
    ) -> Result<(), NativeError> {
        let place = self.path(path)?;
        let old = self.read_place(&place, crate::access_mode::Missing::Warn)?;
        let mut diagnostics = self.diagnostic_reporter.take_operator();
        let value = if matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement) {
            phera_value::increment(&old, &mut diagnostics)
        } else {
            phera_value::decrement(&old, &mut diagnostics)
        };
        self.diagnostics_flush(diagnostics);
        let value = value?;
        self.write_place(&place, value.clone(), crate::access_mode::Binding::ByValue)?;
        self.set(
            destination,
            if matches!(mode, IncDec::PostIncrement | IncDec::PostDecrement) {
                old
            } else {
                value
            },
        );
        Ok(())
    }

    /// Implements PHP AssignPath semantics outside the dispatch loop.
    pub(crate) fn assign_path(&mut self, path: PathId, source: Reg) -> Result<(), NativeError> {
        let value = self.read_local(source);
        self.write_place(
            &self.path(path)?,
            value,
            crate::access_mode::Binding::ByValue,
        )?;
        Ok(())
    }

    /// Implements PHP MakeReference semantics outside the dispatch loop.
    pub(crate) fn make_reference(
        &mut self,
        destination: Reg,
        path: PathId,
    ) -> Result<(), NativeError> {
        let value = self.reference(&self.path(path)?)?;
        self.set(destination, value);
        Ok(())
    }

    /// Implements PHP BindReference semantics outside the dispatch loop.
    pub(crate) fn bind_reference(&mut self, path: PathId, source: Reg) -> Result<(), NativeError> {
        let value = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(source.index()))
            .and_then(Option::as_ref)
            .cloned()
            .unwrap_or_default();
        self.write_place(
            &self.path(path)?,
            value,
            crate::access_mode::Binding::ByReference,
        )?;
        Ok(())
    }
}
