//! Writes through overloaded elements preserve PHP's get/set and reference rules.
//!
//! ```php
//! $store[$key] += 1;   // get followed by set
//! $store[$key]['x'] = 3; // only the returned reference or object can change
//! ```

use std::{cell::RefCell, rc::Rc};

use phera_bytecode::{BinaryOp, IncDec, Instruction};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::Value;

use crate::{
    AccessPath, Vm,
    access_mode::{Creation, Missing, Mutation},
    dimensions::DimensionPath,
};

enum Write {
    Assign(Value),
    Bind(Value),
    Unset,
    Compound(BinaryOp, Value),
    Increment(IncDec),
    Reference,
}

impl Vm {
    #[cold]
    pub(crate) fn prepare_dimension_instruction(
        &mut self,
        instruction: Instruction,
    ) -> Result<bool, NativeError> {
        let path = match instruction {
            Instruction::AssignPathFromLocal { path, .. }
            | Instruction::MakeReference { path, .. }
            | Instruction::BindReference { path, .. }
            | Instruction::SendPath { path }
            | Instruction::UnsetPath { path } => self.path(path)?,
            Instruction::AppendLocal { local, .. } => AccessPath::local(local).with_append(),
            _ => return Ok(false),
        };
        self.prepare_dimension(&path)
    }

    #[cold]
    pub(crate) fn prepare_dimension(&mut self, place: &AccessPath) -> Result<bool, NativeError> {
        if !place.steps()?.any(|step| {
            matches!(
                step,
                phera_bytecode::Step::Index(_) | phera_bytecode::Step::Append
            )
        }) {
            return Ok(false);
        }
        let Some(register) = self.base_register(place, Creation::Existing, Missing::Silent)? else {
            return Ok(false);
        };
        self.pending_dimension = self.find_dimension(self.get(register), place.steps()?)?;
        Ok(self.pending_dimension.is_some())
    }

    pub(crate) async fn dispatch_dimension(
        &mut self,
        instruction: Instruction,
    ) -> Result<(), NativeError> {
        let path = self
            .pending_dimension
            .take()
            .ok_or_else(|| internal_error!("missing overloaded dimension"))?;
        let (write, destination) = match instruction {
            Instruction::Fetch { dst, mode, .. } | Instruction::FetchIndex { dst, mode, .. } => {
                let value = self.fetch_dimension(path, mode).await?;
                self.set(dst, value);
                return Ok(());
            }
            Instruction::AssignPath { src, .. } => (Write::Assign(self.get(src)), None),
            Instruction::AssignPathFromLocal { src, dst, .. } => {
                let value = self.read_root(src, Missing::Warn);
                self.write_dimension_path(path, Write::Assign(value.clone()))
                    .await?;
                self.set(dst, value);
                return Ok(());
            }
            Instruction::AppendLocal { src, .. } => (Write::Assign(self.read_local(src)), None),
            Instruction::AssignOpPath { op, dst, src, .. } => {
                (Write::Compound(op, self.get(src)), Some(dst))
            }
            Instruction::IncDecPath { mode, dst, .. } => (Write::Increment(mode), Some(dst)),
            Instruction::MakeReference { dst, .. } => (Write::Reference, Some(dst)),
            Instruction::BindReference { src, .. } => {
                let value = self
                    .current()
                    .and_then(|frame| frame.slots(&self.registers).get(src.index()))
                    .and_then(Option::as_ref)
                    .cloned()
                    .unwrap_or_default();
                (Write::Bind(value), None)
            }
            Instruction::UnsetPath { .. } => (Write::Unset, None),
            Instruction::SendPath { .. } => {
                self.send_dimension(path).await?;
                return Ok(());
            }
            _ => return Err(internal_error!("overloaded dimension instruction")),
        };
        let value = self.write_dimension_path(path, write).await?;
        if let Some(destination) = destination {
            self.set(destination, value);
        }
        Ok(())
    }

    pub(crate) async fn mutate_property_dimension(
        &mut self,
        path: DimensionPath,
        mutation: crate::magic_mutations::PropertyMutation,
    ) -> NativeResult {
        use crate::access_mode::Binding;
        use crate::magic_mutations::PropertyMutation;
        let write = match mutation {
            PropertyMutation::Write(value, Binding::ByValue) => {
                Write::Assign(value.into_value(self))
            }
            PropertyMutation::Write(value, Binding::ByReference) => {
                Write::Bind(value.into_value(self))
            }
            PropertyMutation::Unset => Write::Unset,
        };
        self.write_dimension_path(path, write).await
    }

    pub(crate) async fn reference_property_dimension(
        &mut self,
        path: DimensionPath,
    ) -> NativeResult {
        self.write_dimension_path(path, Write::Reference).await
    }

    async fn send_dimension(&mut self, path: DimensionPath) -> Result<(), NativeError> {
        let view = crate::calls::call_view::CallView::top(&self.code, &self.call_stack)?;
        let index = view.argument_index()?;
        let parameters = view.parameters();
        let reference = parameters
            .receiving(index)
            .is_some_and(|parameter| parameters.by_reference(parameter))
            || (index == 0 && view.is_extract());
        let value = if reference {
            self.write_dimension_path(path, Write::Reference).await?
        } else {
            self.fetch_dimension(path, phera_bytecode::FetchMode::Read)
                .await?
        };
        self.store_sent_value(index, value)
    }

    async fn write_dimension_path(
        &mut self,
        mut path: DimensionPath,
        write: Write,
    ) -> NativeResult {
        loop {
            if path.suffix.is_empty() {
                return self.write_dimension(path, write).await;
            }
            let returned = self.dimension_get(path.object.clone(), path.offset).await?;
            if !returned.is_reference() && !returned.is_object() {
                self.indirect_dimension_notice(&path.object);
            }
            if let Some(next) =
                self.find_dimension(returned.deref().clone(), path.suffix.iter().copied())?
            {
                path = next;
                continue;
            }
            return self.mutate_returned_value(returned, &path.suffix, write);
        }
    }

    async fn write_dimension(&mut self, path: DimensionPath, write: Write) -> NativeResult {
        let DimensionPath { object, offset, .. } = path;
        match write {
            Write::Assign(value) => {
                self.dimension_set(object, offset, value).await?;
                Ok(Value::Null)
            }
            Write::Unset => {
                self.dimension_unset(object, offset).await?;
                Ok(Value::Null)
            }
            Write::Compound(operator, right) => {
                let left = self.dimension_get(object.clone(), offset.clone()).await?;
                let result = self.dimension_binary(operator, &left, &right)?;
                self.dimension_set(object, offset, result.clone()).await?;
                Ok(result)
            }
            write => {
                let value = self.dimension_get(object.clone(), offset).await?;
                if !value.is_reference() && !value.is_object() {
                    self.indirect_dimension_notice(&object);
                }
                match write {
                    Write::Bind(_) => Err(NativeError::error(
                        "Cannot assign by reference to an array dimension of an object",
                    )),
                    Write::Reference => Ok(make_cell(value)),
                    Write::Increment(mode) => {
                        let (updated, result) = self.increment_dimension(&value, mode)?;
                        if let Value::Reference(reference) = value {
                            let updated = self.check_reference_assignment(&reference, updated)?;
                            *reference
                                .try_borrow_mut()
                                .map_err(|_| internal_error!("borrowed dimension"))? = updated;
                        }
                        Ok(result)
                    }
                    _ => Err(internal_error!("overloaded dimension write")),
                }
            }
        }
    }

    fn mutate_returned_value(
        &mut self,
        mut root: Value,
        steps: &[phera_bytecode::Step],
        write: Write,
    ) -> NativeResult {
        let plain = root.deref().clone();
        let read = |vm: &mut Self| {
            steps.iter().try_fold(plain.clone(), |value, step| {
                vm.ordinary_step(value, *step, Missing::Warn)
            })
        };
        let keys = self.property_keys_for(steps.iter().copied(), plain.clone())?;
        let (mut value, mutation, result) = match write {
            Write::Assign(value) => (value, Mutation::Assign, Value::Null),
            Write::Bind(value) => (value, Mutation::Bind, Value::Null),
            Write::Unset => (Value::Null, Mutation::Unset, Value::Null),
            Write::Compound(operator, rhs) => {
                let old = read(self)?;
                let value = self.dimension_binary(operator, &old, &rhs)?;
                (value.clone(), Mutation::Assign, value)
            }
            Write::Increment(mode) => {
                let old = read(self)?;
                let (value, result) = self.increment_dimension(&old, mode)?;
                (value, Mutation::Assign, result)
            }
            Write::Reference => {
                let existing = raw_member(&root, &keys);
                if existing.is_reference() {
                    return Ok(existing);
                }
                let value = make_cell(existing);
                (value.clone(), Mutation::Bind, value)
            }
        };
        if mutation == Mutation::Assign {
            value = self.check_path_reference_assignment(&root, &keys, value)?;
        }
        self.validate_property_mutation(plain, &keys, &mut value, mutation)?;
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        crate::path_update::update(
            &mut root,
            &keys,
            value,
            mutation,
            &mut diagnostics.buffer,
            &diagnostics.vm.property_references,
        )?;
        Ok(result)
    }

    fn dimension_binary(
        &mut self,
        operator: BinaryOp,
        left: &Value,
        right: &Value,
    ) -> NativeResult {
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        Ok(crate::ops::binary(
            operator,
            &left.deref(),
            &right.deref(),
            &mut diagnostics.buffer,
        )?)
    }

    fn increment_dimension(
        &mut self,
        original: &Value,
        mode: IncDec,
    ) -> Result<(Value, Value), NativeError> {
        let original = original.deref().clone();
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let updated = if matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement) {
            phera_value::increment(&original, &mut diagnostics.buffer)?
        } else {
            phera_value::decrement(&original, &mut diagnostics.buffer)?
        };
        let result = if matches!(mode, IncDec::PostIncrement | IncDec::PostDecrement) {
            original
        } else {
            updated.clone()
        };
        Ok((updated, result))
    }
}

/// A reference to an overloaded array's member must keep any existing cell.
fn raw_member(
    root: &Value,
    keys: &crate::inline_stack::InlineStack<crate::path_update::PathKey>,
) -> Value {
    use crate::path_update::PathKey;
    let mut current = root.clone();
    for key in keys.iter() {
        let member = match (&*current.deref(), key) {
            (Value::Array(array), PathKey::Index(Some(key))) => array.get(key).cloned(),
            (Value::Object(object), PathKey::Property(key)) => {
                crate::property_storage::read(object, key)
            }
            _ => None,
        };
        current = member.unwrap_or_default();
    }
    current
}

fn make_cell(value: Value) -> Value {
    if value.is_reference() {
        value
    } else {
        Value::Reference(Rc::new(RefCell::new(value)))
    }
}
