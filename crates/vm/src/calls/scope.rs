//! Dynamic scope grows locals before any reserved call windows.

use crate::{Vm, intrinsics::Intrinsic};
use phera_bytecode::Reg;
use phera_extension_api::NativeError;

impl Vm {
    /// Dynamic-scope operations reuse declared locals before allocating a new named slot.
    pub(crate) fn named_slot(&mut self, name: &str) -> Result<Reg, NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("no variable frame"))?;
        if let Some(i) = frame.function().locals.iter().position(|candidate| {
            frame
                .unit()
                .names
                .get(candidate.index())
                .is_some_and(|value| value == name)
        }) {
            return Ok(Reg(i as u16));
        }
        if let Some(register) = frame.extras.dynamic.get(name) {
            return Ok(*register);
        }
        let count = frame.count();
        let register = Reg(u16::try_from(count)
            .map_err(|_| NativeError::error("Phera register limit exceeded"))?);
        let insertion = frame.base() + count;
        self.registers.insert_undefined(insertion);
        let extras = self.call_stack.current_extras_mut();
        if let Some(base) = &mut extras.arguments.saved {
            *base += 1;
        }
        extras.dynamic_count = Some(count + 1);
        extras.dynamic.insert(name.into(), register);
        for call in &mut self.call_stack.preparing {
            if call.window.base >= insertion {
                call.window.base += 1;
            }
        }
        Ok(register)
    }

    /// Namespace fallbacks test user declarations, natives and VM intrinsics.
    pub(crate) fn exists(&self, name: &str) -> bool {
        self.function_table.contains(&name.to_ascii_lowercase())
            || self.registry.get_function(name).is_some()
            || Intrinsic::resolve(name).is_some()
    }
}
