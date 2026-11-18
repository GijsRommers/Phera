//! PHP path instructions instruction handlers, separate from dispatch.

use crate::calls::execution_loop::BatchExit;
use phera_bytecode::{BinaryOp, FetchMode, IncDec, Instruction, PathId, Reg};
use phera_extension_api::NativeError;

use crate::{
    AccessPath, Vm,
    access_mode::{Binding, Creation, Missing},
    inline_stack::InlineStack,
    ops,
    path_update::PathKey,
};
use phera_value::Value;

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
        if mode == FetchMode::Read && self.fetch_property_site(&place, destination)? {
            return Ok(None);
        }
        if self.classes.has_dimensions && self.prepare_dimension(&place)? {
            return Ok(Some(BatchExit::Instruction(Instruction::Fetch {
                dst: destination,
                path,
                mode,
            })));
        }
        if place.has_properties()? && mode != FetchMode::Destructure {
            if mode != FetchMode::Read
                && (!matches!(place.base()?, phera_bytecode::PathBase::This { .. })
                    || self.current().is_some_and(|frame| frame.this().is_some()))
                && self.start_cached_getter(&place, destination)?
            {
                return Ok(None);
            }
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
            match mode {
                FetchMode::Read | FetchMode::Destructure => crate::access_mode::Missing::Warn,
                FetchMode::Quiet => crate::access_mode::Missing::Silent,
                FetchMode::Isset | FetchMode::Empty => crate::access_mode::Missing::Isset,
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
        if (self.classes.has_dimensions && self.prepare_dimension(&place)?)
            || place.has_properties()?
        {
            return Ok(Some(BatchExit::Instruction(Instruction::AssignOpPath {
                op: operator,
                dst: destination,
                path,
                src: source,
            })));
        }
        let (register, keys, left) = self.read_indexed_update(&place)?;
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let value = ops::binary(
            operator,
            &left,
            &diagnostics.vm.operand(source),
            &mut diagnostics.buffer,
        );
        drop(diagnostics);
        let value = value?;
        self.finish_indexed_update(&place, register, keys, value.clone())?;
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
        if (self.classes.has_dimensions && self.prepare_dimension(&place)?)
            || place.has_properties()?
        {
            return Ok(Some(BatchExit::Instruction(Instruction::IncDecPath {
                mode,
                dst: destination,
                path,
            })));
        }
        let (register, keys, old) = self.read_indexed_update(&place)?;
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let value = if matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement) {
            phera_value::increment(&old, &mut diagnostics.buffer)
        } else {
            phera_value::decrement(&old, &mut diagnostics.buffer)
        };
        drop(diagnostics);
        let value = value?;
        self.finish_indexed_update(&place, register, keys, value.clone())?;
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

    /// Resolve a writable index once for both halves of a compound update.
    fn read_indexed_update(
        &mut self,
        place: &AccessPath,
    ) -> Result<(Option<Reg>, InlineStack<PathKey>, Value), NativeError> {
        let register = self.base_register(place, Creation::Existing, Missing::Warn)?;
        self.check_static_read(place, Missing::Warn)?;
        let mut value = register.map_or(Value::Null, |register| {
            self.read_root(register, Missing::Warn)
        });
        let mut keys = InlineStack::new();
        for step in place.steps()? {
            let phera_bytecode::Step::Index(index) = step else {
                return Err(NativeError::error("Cannot use [] for reading"));
            };
            let key = self.write_index_key(&value, index)?;
            value = self.read_writable_index(value, &key)?;
            keys.push(PathKey::Index(Some(key)));
        }
        Ok((register, keys, value))
    }

    fn finish_indexed_update(
        &mut self,
        place: &AccessPath,
        register: Option<Reg>,
        keys: InlineStack<PathKey>,
        value: Value,
    ) -> Result<(), NativeError> {
        if keys.iter().next().is_none() {
            self.write_place(place, value, Binding::ByValue)
        } else {
            let register = match register {
                Some(register) => register,
                None => self
                    .base_register(place, Creation::Create, Missing::Silent)?
                    .ok_or_else(|| internal_error!("missing indexed update"))?,
            };
            self.write_resolved_place(place, register, keys, value, Binding::ByValue)
        }
    }

    /// Implements PHP AssignPath semantics outside the dispatch loop.
    pub(crate) fn assign_path(
        &mut self,
        path: PathId,
        source: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let value = self.read_local(source);
        let place = self.path(path)?;
        if self.assign_property_site(&place, &value, None)? {
            return Ok(None);
        }
        if place.steps()?.next().is_none()
            && let phera_bytecode::PathBase::Register(register) = place.base()?
        {
            self.restart_subject_iterators(register);
        }
        if (self.classes.has_dimensions && self.prepare_dimension(&place)?)
            || place.has_properties()?
            || (self.may_cast_typed_reference(&value) && place.steps()?.next().is_some())
            || (matches!(value, phera_value::Value::Object(_))
                && matches!(
                    place.base()?,
                    phera_bytecode::PathBase::StaticProperty { .. }
                )
                && place.steps()?.next().is_none()
                && Self::has_string_conversion(&value))
        {
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
