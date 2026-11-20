//! Cold instruction dispatch delegates PHP behavior to concern-specific handlers.

use phera_bytecode::{BinaryOp, Instruction};
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
        use crate::property_hook_continuation::{
            ReadCompletion, ReferenceCompletion, WriteCompletion,
        };

        if self.pending_dimension.is_some() {
            return self.dispatch_object_operation(instruction).await;
        }

        match instruction {
            Instruction::InitUserCallback {
                callback,
                arguments,
            } => {
                Box::pin(self.autoload_user_callback(callback, arguments)).await?;
            }
            Instruction::GeneratorReturn { src } => {
                return self
                    .dispatched_return(
                        crate::calls::returns::DispatchedReturn::Generator(src),
                        stop,
                    )
                    .await;
            }
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
                // Zend emits `right < left` for `left > right` after
                // compiling both operand expressions in source order.
                let (left_missing, right_missing) =
                    if matches!(op, BinaryOp::Greater | BinaryOp::GreaterEqual) {
                        let right = self.warn_missing_operand(rhs).await?;
                        (self.warn_missing_operand(lhs).await?, right)
                    } else {
                        let left = self.warn_missing_operand(lhs).await?;
                        (left, self.warn_missing_operand(rhs).await?)
                    };
                let left = if left_missing {
                    Value::Null
                } else {
                    self.get(lhs)
                };
                let right = if right_missing {
                    Value::Null
                } else {
                    self.get(rhs)
                };
                let value = self.binary_with_strings(op, left, right).await?;
                self.set(dst, value);
            }
            Instruction::AssignOp { op, local, src } => {
                // PHP reads the right side first, so when both are
                // undefined its warning comes first.
                let right = self.read_local(src);
                let left = self.read_local(local);
                // An "Undefined variable" warning reaches the script's error
                // handler before the result is stored. The handler may unset
                // or assign the variable; the result still replaces it.
                if !self.error_handlers.pending.is_empty() {
                    self.drain_error_handlers().await?;
                }
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
                self.read_property_path(self.path(path)?, mode, ReadCompletion::Fetch(dst))
                    .await?;
            }
            Instruction::SendPath { path } => self.send_magic_place(&self.path(path)?).await?,
            Instruction::AssignPath { path, src } => {
                let place = self.path(path)?;
                let value = self.get(src).deref().clone();
                if self.may_cast_typed_reference(&value)
                    && place.steps()?.next().is_some()
                    && !place.has_properties()?
                {
                    if let Some(value) = self.assign_indexed_stringable(&place, value).await? {
                        self.set_cast_assignment_result(src, value);
                    }
                    return Ok(DispatchResult::Continue);
                }
                let static_cast = self.cast_static_property_value(&place, &value).await?;
                self.mutate_property_path(
                    place,
                    PropertyMutation::Write(
                        WriteValue::evaluated(static_cast.clone().unwrap_or(value)),
                        crate::access_mode::Binding::ByValue,
                    ),
                    WriteCompletion::Cast(src, static_cast),
                )
                .await?;
            }
            Instruction::AssignPathFromLocal { path, src, dst } => {
                self.mutate_property_path(
                    self.path(path)?,
                    PropertyMutation::Write(
                        WriteValue::local(src, dst),
                        crate::access_mode::Binding::ByValue,
                    ),
                    WriteCompletion::Local(dst),
                )
                .await?;
            }
            Instruction::AssignOpPath { op, dst, path, src } => {
                self.read_property_path(
                    self.path(path)?,
                    phera_bytecode::FetchMode::Read,
                    ReadCompletion::AssignOp(op, dst, src),
                )
                .await?
            }
            Instruction::IncDecPath { mode, dst, path } => {
                self.read_property_path(
                    self.path(path)?,
                    phera_bytecode::FetchMode::Read,
                    ReadCompletion::IncDec(mode, dst),
                )
                .await?
            }
            Instruction::MakeReference { dst, path } => {
                self.reference_property_path(self.path(path)?, ReferenceCompletion::Fetch(dst))
                    .await?;
            }
            Instruction::BindReference { path, src } => {
                let value = self.raw_local(src);
                self.mutate_property_path(
                    self.path(path)?,
                    PropertyMutation::Write(
                        WriteValue::evaluated(value),
                        crate::access_mode::Binding::ByReference,
                    ),
                    WriteCompletion::Discard,
                )
                .await?;
            }
            Instruction::UnsetPath { path } => {
                self.mutate_property_path(
                    self.path(path)?,
                    PropertyMutation::Unset,
                    WriteCompletion::Discard,
                )
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
                    let subject = self.get(src);
                    if let Value::Object(object) = &subject
                        && object.lazy().is_some()
                    {
                        self.lazy_iterator(iter, None, object.clone()).await?;
                        return Ok(DispatchResult::Continue);
                    }
                    if matches!(&subject, Value::Object(object) if crate::generators::generator_object(object))
                    {
                        self.discard_generator_temporary(src);
                    }
                    self.start_traversable(iter, subject, false).await?;
                }
                Instruction::IterStartReference {
                    iter,
                    subject,
                    path,
                    ..
                } => {
                    let place = self.path(path)?;
                    if place.has_properties()? {
                        self.reference_property_path(
                            place,
                            crate::property_hook_continuation::ReferenceCompletion::Foreach(
                                iter, subject,
                            ),
                        )
                        .await?;
                    } else {
                        let value = self.get(subject);
                        if let Value::Object(object) = &value
                            && object.lazy().is_some()
                        {
                            self.lazy_iterator(iter, Some(subject), object.clone())
                                .await?;
                            return Ok(DispatchResult::Continue);
                        }
                        self.set(subject, Value::Null);
                        self.start_traversable(iter, value, true).await?;
                    }
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
