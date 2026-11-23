//! Compound and increment instructions perform a read followed by a write
//! when a property is overloaded, for example `$object->counter++`.

use phera_bytecode::{BinaryOp, IncDec, Reg};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::access_mode::{Binding, Mutation};
use crate::magic_mutations::{PropertyMutation, WriteValue};
use crate::magic_places::{ReadPlace, UpdateTarget};
use crate::property_hook_continuation::{HookContinuation, WriteCompletion};
use crate::{AccessPath, Vm};

impl Vm {
    pub(crate) async fn finish_magic_assign_op(
        &mut self,
        place: &AccessPath,
        left: ReadPlace,
        operator: BinaryOp,
        destination: Reg,
        source: Reg,
    ) -> Result<(), NativeError> {
        let value = self
            .binary_with_strings(operator, left.value, self.get(source))
            .await?;
        self.finish_path_update(
            place,
            left.target,
            value.clone(),
            WriteCompletion::Update(destination, value),
        )
        .await
    }

    pub(crate) async fn finish_magic_inc_dec(
        &mut self,
        place: &AccessPath,
        target: ReadPlace,
        mode: IncDec,
        destination: Reg,
    ) -> Result<(), NativeError> {
        let old = target.value;
        let value = {
            let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
            match mode {
                IncDec::PreIncrement | IncDec::PostIncrement => {
                    phera_value::increment(&old, &mut diagnostics.buffer)?
                }
                IncDec::PreDecrement | IncDec::PostDecrement => {
                    phera_value::decrement(&old, &mut diagnostics.buffer)?
                }
            }
        };
        if let Some(limit) = crate::typed_increment::IntegerLimit::reached(&old, &value, mode) {
            match &target.target {
                Some(UpdateTarget::Resolved { register, keys }) => {
                    self.check_increment_place(place, *register, keys, limit)?;
                }
                Some(UpdateTarget::Property { object, key }) => {
                    self.check_increment_property(object, key, limit)?;
                }
                Some(UpdateTarget::Indirect { root, keys }) => {
                    self.check_increment_target(root, keys, limit)?;
                }
                _ => {}
            }
        }
        let result = match mode {
            IncDec::PostIncrement | IncDec::PostDecrement => old,
            IncDec::PreIncrement | IncDec::PreDecrement => value.clone(),
        };
        self.finish_path_update(
            place,
            target.target,
            value,
            WriteCompletion::Update(destination, result),
        )
        .await
    }

    /// An indirect update writes through the original __get result, including
    /// its reference identity. Rewalking the PHP path would run __get again.
    async fn finish_path_update(
        &mut self,
        place: &AccessPath,
        target: Option<UpdateTarget>,
        value: Value,
        completion: WriteCompletion,
    ) -> Result<(), NativeError> {
        match target {
            Some(UpdateTarget::Resolved { register, keys }) => {
                let current = self.read_root(register, crate::access_mode::Missing::Silent);
                let mut value = value;
                if !self.validate_property_mutation(current, &keys, &mut value, Mutation::Assign)? {
                    self.write_resolved_place(place, register, *keys, value, Binding::ByValue)?;
                }
            }
            Some(UpdateTarget::Indirect { root, keys }) => {
                self.write_detached_place(root, &keys, value, Mutation::Assign)
                    .await?;
            }
            Some(UpdateTarget::Dimension(object, offset)) => {
                self.dimension_set(object, offset, value).await?;
            }
            Some(UpdateTarget::Property { object, key }) => {
                if let Some(setter) = self
                    .plan_selected_property_write(object, key, value)
                    .await?
                {
                    return self
                        .suspend_for_hook(
                            setter.function,
                            setter.object,
                            Some(setter.argument),
                            HookContinuation::Written {
                                local: None,
                                completion,
                            },
                        )
                        .await;
                }
            }
            None => {
                return self
                    .mutate_property_path(
                        place.clone(),
                        PropertyMutation::Write(WriteValue::evaluated(value), Binding::ByValue),
                        completion,
                    )
                    .await;
            }
        }
        completion.finish(self, None);
        Ok(())
    }
}
