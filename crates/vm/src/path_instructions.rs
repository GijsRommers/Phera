//! PHP path instructions instruction handlers, separate from dispatch.

use crate::calls::execution_loop::BatchExit;
use phera_bytecode::{BinaryOp, FetchMode, IncDec, Instruction, PathId, Reg};
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
    ) -> Result<Option<BatchExit>, NativeError> {
        let place = self.path(path)?;
        if mode == FetchMode::Read
            && let Some(value) = self.read_property_site(&place)?
        {
            self.set(destination, value);
            return Ok(None);
        }
        if place.has_properties()? && mode != FetchMode::Destructure {
            return Ok(Some(BatchExit::Instruction(Instruction::Fetch {
                dst: destination,
                path,
                mode,
            })));
        }
        if mode == FetchMode::Destructure {
            let root = self.read_place(
                &crate::AccessPath::local(match place.base()? {
                    phera_bytecode::PathBase::Register(register) => register,
                    _ => return Err(internal_error!("destructure root")),
                }),
                crate::access_mode::Missing::Silent,
            )?;
            if !root.is_array() {
                if !root.is_null() {
                    self.warn(format!("Cannot use {} as array", root.type_name()));
                }
                self.set(destination, phera_value::Value::Null);
                return Ok(None);
            }
        }
        let value = self.read_place(
            &place,
            if matches!(mode, FetchMode::Read | FetchMode::Destructure) {
                crate::access_mode::Missing::Warn
            } else {
                crate::access_mode::Missing::Silent
            },
        )?;
        self.set(destination, value);
        Ok(None)
    }

    /// Implements PHP AssignOpPath semantics outside the dispatch loop.
    pub(crate) fn assign_op_path(
        &mut self,
        operator: BinaryOp,
        destination: Reg,
        path: PathId,
        source: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let place = self.path(path)?;
        if place.has_properties()? {
            return Ok(Some(BatchExit::Instruction(Instruction::AssignOpPath {
                op: operator,
                dst: destination,
                path,
                src: source,
            })));
        }
        let left = self.read_place(&place, crate::access_mode::Missing::Warn)?;
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let value = ops::binary(
            operator,
            &left,
            &diagnostics.vm.operand(source),
            &mut diagnostics.buffer,
        );
        drop(diagnostics);
        let value = value?;
        self.write_place(&place, value.clone(), crate::access_mode::Binding::ByValue)?;
        self.set(destination, value);
        Ok(None)
    }

    /// Implements PHP IncDecPath semantics outside the dispatch loop.
    pub(crate) fn inc_dec_path(
        &mut self,
        mode: IncDec,
        destination: Reg,
        path: PathId,
    ) -> Result<Option<BatchExit>, NativeError> {
        let place = self.path(path)?;
        if let Some(value) = self.increment_property_site(&place, mode)? {
            self.set(destination, value);
            return Ok(None);
        }
        if place.has_properties()? {
            return Ok(Some(BatchExit::Instruction(Instruction::IncDecPath {
                mode,
                dst: destination,
                path,
            })));
        }
        let old = self.read_place(&place, crate::access_mode::Missing::Warn)?;
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let value = if matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement) {
            phera_value::increment(&old, &mut diagnostics.buffer)
        } else {
            phera_value::decrement(&old, &mut diagnostics.buffer)
        };
        drop(diagnostics);
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
        Ok(None)
    }

    /// Implements PHP AssignPath semantics outside the dispatch loop.
    pub(crate) fn assign_path(
        &mut self,
        path: PathId,
        source: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let value = self.read_local(source);
        let place = self.path(path)?;
        if self.assign_property_site(&place, &value)? {
            return Ok(None);
        }
        if place.steps()?.next().is_none()
            && let phera_bytecode::PathBase::Register(register) = place.base()?
        {
            self.restart_subject_iterators(register);
        }
        if place.has_properties()? {
            return Ok(Some(BatchExit::Instruction(Instruction::AssignPath {
                path,
                src: source,
            })));
        }
        self.write_place(&place, value, crate::access_mode::Binding::ByValue)?;
        Ok(None)
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
        let value = self.raw_local(source);
        self.write_place(
            &self.path(path)?,
            value,
            crate::access_mode::Binding::ByReference,
        )?;
        Ok(())
    }
    /// Reference-binding instructions need the slot itself, before dereferencing.
    pub(crate) fn raw_local(&self, source: Reg) -> phera_value::Value {
        self.current()
            .and_then(|frame| frame.slots(&self.registers).get(source.index()))
            .and_then(Option::as_ref)
            .cloned()
            .unwrap_or_default()
    }
}
