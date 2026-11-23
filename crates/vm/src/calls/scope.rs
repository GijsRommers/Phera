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
        if let Some(register) = frame.named_register(name) {
            return Ok(register);
        }
        let count = frame.count();
        let register = Reg(u16::try_from(count)
            .map_err(|_| NativeError::error("Phera register limit exceeded"))?);
        let insertion = frame.base() + count;
        let value = if self.in_global_scope() {
            self.globals.declare(name);
            self.globals.take(name)
        } else {
            None
        };
        self.registers.insert_undefined(insertion);
        self.registers.window_mut(insertion, 1)[0] = value;
        let extras = self.call_stack.current_extras_mut();
        if let Some(base) = &mut extras.arguments.saved {
            *base += 1;
        }
        extras.dynamic_count = Some(count + 1);
        extras.dynamic.insert(name.into(), register);
        self.call_stack.shift_calls_from(insertion);
        Ok(register)
    }

    /// Namespace fallbacks test user declarations, natives and VM intrinsics.
    pub(crate) fn exists(&self, name: &str) -> bool {
        let name = phera_extension_api::folded_name(name);
        self.function_table.contains(&name)
            || self.registry.get_folded_function(&name).is_some()
            || Intrinsic::resolve_folded(&name).is_some()
    }
}
