//! Compound and increment instructions perform a read followed by a write
//! when a property is overloaded, for example `$object->counter++`.

use phera_bytecode::{BinaryOp, FetchMode, IncDec, Reg};
use phera_extension_api::NativeError;

use crate::access_mode::Binding;
use crate::magic_mutations::PropertyMutation;
use crate::{AccessPath, Vm};

impl Vm {
    pub(crate) async fn magic_assign_op_path(
        &mut self,
        place: &AccessPath,
        operator: BinaryOp,
        destination: Reg,
        source: Reg,
    ) -> Result<(), NativeError> {
        let left = self.read_magic_place(place, FetchMode::Read).await?;
        let value = self
            .binary_with_strings(operator, left, self.get(source))
            .await?;
        self.mutate_magic_place(
            place,
            PropertyMutation::Write(value.clone(), Binding::ByValue),
        )
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
        let old = self.read_magic_place(place, FetchMode::Read).await?;
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
        self.mutate_magic_place(
            place,
            PropertyMutation::Write(value.clone(), Binding::ByValue),
        )
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
}
