//! Compound and increment instructions perform a read followed by a write
//! when a property is overloaded, for example `$object->counter++`.

use phera_bytecode::{BinaryOp, IncDec, Reg};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::access_mode::{Binding, Mutation};
use crate::magic_mutations::{PropertyMutation, WriteValue};
use crate::magic_places::UpdateTarget;
use crate::{AccessPath, Vm};

impl Vm {
    pub(crate) async fn magic_assign_op_path(
        &mut self,
        place: &AccessPath,
        operator: BinaryOp,
        destination: Reg,
        source: Reg,
    ) -> Result<(), NativeError> {
        let left = self.read_for_path_update(place).await?;
        let value = self
            .binary_with_strings(operator, left.value, self.get(source))
            .await?;
        self.finish_path_update(place, left.target, value.clone())
            .await?;
        self.set(destination, value);
        Ok(())
    }

    pub(crate) async fn magic_inc_dec_path(
        &mut self,
        place: &AccessPath,
        mode: IncDec,
        destination: Reg,
    ) -> Result<(), NativeError> {
        let target = self.read_for_path_update(place).await?;
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
        self.finish_path_update(place, target.target, value.clone())
            .await?;
        self.set(
            destination,
            match mode {
                IncDec::PostIncrement | IncDec::PostDecrement => old,
                IncDec::PreIncrement | IncDec::PreDecrement => value,
            },
        );
        Ok(())
    }

    /// An indirect update writes through the original __get result, including
    /// its reference identity. Rewalking the PHP path would run __get again.
    async fn finish_path_update(
        &mut self,
        place: &AccessPath,
        target: Option<UpdateTarget>,
        value: Value,
    ) -> Result<(), NativeError> {
        match target {
            Some(UpdateTarget::Indirect { root, keys }) => self
                .write_detached_place(root, &keys, value, Mutation::Assign)
                .await
                .map(|_| ()),
            Some(UpdateTarget::Property { object, key }) => {
                self.assign_selected_property(object, key, value).await
            }
            None => self
                .mutate_magic_place(
                    place,
                    PropertyMutation::Write(WriteValue::evaluated(value), Binding::ByValue),
                )
                .await
                .map(|_| ()),
        }
    }
}
