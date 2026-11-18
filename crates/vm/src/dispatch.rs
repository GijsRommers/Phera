//! Cold instruction dispatch delegates PHP behavior to concern-specific handlers.

use phera_bytecode::Instruction;
use phera_extension_api::{LocalFuture, NativeError};
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
    ) -> Result<DispatchResult, NativeError> {
        use crate::magic_mutations::{PropertyMutation, WriteValue};

        if self.pending_dimension.is_some() {
            return self.dispatch_object_operation(instruction).await;
        }

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

            Instruction::Echo { src } | Instruction::Cast { src, .. } => {
                let value = self.get(src);
                let string = self.stringify_value(&value).await?;
                if let Instruction::Cast { dst, .. } = instruction {
                    self.set(dst, Value::String(string));
                } else {
                    self.out(string.as_bytes());
                }
            }
            Instruction::Binary { dst, op, lhs, rhs } => {
                let value = self
                    .binary_with_strings(op, self.get(lhs), self.get(rhs))
                    .await?;
                self.set(dst, value);
            }
            Instruction::AssignOp { op, local, src } => {
                let left = self.read_local(local);
                let right = self.read_local(src);
                let value = self.binary_with_strings(op, left, right).await?;
                self.assign_local(local, value)?;
            }
            Instruction::Assign { local, src } => {
                let value = self.coerce_stringable(self.get(src)).await?;
                self.restart_subject_iterators(local);
                self.assign_local(local, value.clone())?;
                self.set_cast_assignment_result(src, value);
            }
            Instruction::CheckReturn { source } => {
                let value = self.coerce_stringable(self.raw_local(source)).await?;
                let value = self.check_return_value(value)?;
                self.set(source, value);
            }
            Instruction::Fetch { dst, path, mode }
            | Instruction::FetchIndex {
                dst, path, mode, ..
            } => {
                let value = self.read_magic_place(&self.path(path)?, mode).await?;
                self.set(dst, value);
            }
            Instruction::SendPath { path } => self.send_magic_place(&self.path(path)?).await?,
            Instruction::AssignPath { path, src } => {
                let place = self.path(path)?;
                let value = self.get(src).deref().clone();
                let static_cast = self.cast_static_property_value(&place, &value).await?;
                let written = self
                    .mutate_magic_place(
                        &place,
                        PropertyMutation::Write(
                            WriteValue::evaluated(static_cast.clone().unwrap_or(value)),
                            crate::access_mode::Binding::ByValue,
                        ),
                    )
                    .await?;
                if let Some(value) = written.or(static_cast) {
                    self.set_cast_assignment_result(src, value);
                }
            }
            Instruction::AssignPathFromLocal { path, src, dst } => {
                if let Some(value) = self
                    .mutate_magic_place(
                        &self.path(path)?,
                        PropertyMutation::Write(
                            WriteValue::local(src, dst),
                            crate::access_mode::Binding::ByValue,
                        ),
                    )
                    .await?
                {
                    self.set(dst, value);
                }
            }
            Instruction::AssignOpPath { op, dst, path, src } => {
                self.magic_assign_op_path(&self.path(path)?, op, dst, src)
                    .await?
            }
            Instruction::IncDecPath { mode, dst, path } => {
                self.magic_inc_dec_path(&self.path(path)?, mode, dst)
                    .await?
            }
            Instruction::MakeReference { dst, path } => {
                let value = self.magic_reference(&self.path(path)?).await?;
                self.set(dst, value);
            }
            Instruction::BindReference { path, src } => {
                let value = self.raw_local(src);
                self.mutate_magic_place(
                    &self.path(path)?,
                    PropertyMutation::Write(
                        WriteValue::evaluated(value),
                        crate::access_mode::Binding::ByReference,
                    ),
                )
                .await?;
            }
            Instruction::UnsetPath { path } => {
                self.mutate_magic_place(&self.path(path)?, PropertyMutation::Unset)
                    .await?;
            }
            Instruction::Clone { dst, src, with } => self.clone_instruction(dst, src, with).await?,
            Instruction::Exit { src: source } => {
                return self.exit_value(source).await.map(DispatchResult::Returned);
            }
            Instruction::Eval { dst, source } => self.eval(dst, source).await?,
            Instruction::Include {
                dst: destination,
                path,
                kind,
            } => self.include(destination, path, kind).await?,
            _ => return self.dispatch_object_operation(instruction).await,
        }
        Ok(DispatchResult::Continue)
    }

    /// The PHP interpreter's common suspension state need not hold the futures for object hooks.
    #[cold]
    fn dispatch_object_operation(
        &mut self,
        instruction: Instruction,
    ) -> LocalFuture<'_, Result<DispatchResult, NativeError>> {
        Box::pin(async move {
            if self.pending_dimension.is_some() {
                self.dispatch_dimension(instruction).await?;
                return Ok(DispatchResult::Continue);
            }
            match instruction {
                Instruction::ArraySpread { array, src } => {
                    self.spread_traversable(
                        src,
                        crate::traversable_unpack::SpreadTarget::Array(array),
                    )
                    .await?;
                }
                Instruction::SendSpread { src } => {
                    self.spread_traversable(
                        src,
                        crate::traversable_unpack::SpreadTarget::Arguments,
                    )
                    .await?;
                }
                Instruction::IterStart { iter, src, .. } => {
                    self.start_traversable(iter, self.get(src), false).await?;
                }
                Instruction::IterStartReference { iter, subject, .. } => {
                    let value = self.get(subject);
                    self.set(subject, Value::Null);
                    self.start_traversable(iter, value, true).await?;
                }
                Instruction::IterNext { iter, value, end } => {
                    self.next_traversable(iter, None, value, end).await?;
                }
                Instruction::IterNextKeyed {
                    iter,
                    key,
                    value,
                    end,
                } => {
                    self.next_traversable(iter, Some(key), value, end).await?;
                }
                _ => return Err(internal_error!("unimplemented instruction")),
            }
            Ok(DispatchResult::Continue)
        })
    }
}
