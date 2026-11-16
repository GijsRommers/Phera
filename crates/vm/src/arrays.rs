//! In-place array construction without keeping an extra COW owner.
use crate::Vm;
use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, CollectDiagnostics, Value};
use std::rc::Rc;
impl Vm {
    pub(crate) fn array_write(
        &mut self,
        register: Reg,
        key: Option<Reg>,
        source: Reg,
        spread: bool,
    ) -> Result<(), NativeError> {
        let value = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(source.index()))
            .and_then(Option::as_ref)
            .cloned()
            .unwrap_or_default();
        let mut diagnostics = CollectDiagnostics::default();
        let key = key
            .map(|key| ArrayKey::from_value_diag(&self.get(key), &mut diagnostics))
            .transpose()?;
        self.diagnostics_flush(diagnostics);
        let mut target = self
            .frames
            .last_mut()
            .and_then(|frame| {
                frame
                    .slots_mut(&mut self.registers)
                    .get_mut(register.index())
            })
            .and_then(Option::take)
            .unwrap_or_default();
        let Value::Array(array) = &mut target else {
            return Err(NativeError::error("Phera internal error: array"));
        };
        let array = Rc::make_mut(array);
        if spread {
            let Value::Array(source) = value else {
                return Err(NativeError::error(
                    "Only arrays and Traversables can be unpacked",
                ));
            };
            for (key, value) in source.iter() {
                if matches!(key, ArrayKey::Int(_)) {
                    array.push(value.clone())?;
                } else {
                    array.insert(key.clone(), value.clone());
                }
            }
        } else if let Some(key) = key {
            array.insert(key, value);
        } else {
            array.push(value)?;
        }
        self.set(register, target);
        Ok(())
    }
}
