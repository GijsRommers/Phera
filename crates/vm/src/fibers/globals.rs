//! PHP `global $counter` inside a fiber binds the request's main-script
//! variable even while the main stack is parked.

use phera_bytecode::Reg;

use crate::Vm;

impl Vm {
    pub(super) fn export_main_globals(&mut self) {
        for (register, name) in self.fiber_main_symbols() {
            let value = self.frame_at(0).and_then(|frame| {
                frame
                    .slots(&self.registers)
                    .get(register.index())
                    .cloned()
                    .flatten()
            });
            if let Some(value) = value {
                self.globals.insert(name.clone(), value);
                let reference = self.globals.reference(name);
                if let Some(slot) = self.main_slot_mut(register) {
                    *slot = Some(reference);
                }
            } else {
                self.globals.remove(&name);
            }
        }
    }

    pub(super) fn import_main_globals(&mut self) {
        for (register, name) in self.fiber_main_symbols() {
            let value = self.globals.get(&name).cloned();
            if let Some(slot) = self.main_slot_mut(register) {
                *slot = value;
            }
        }
    }

    fn fiber_main_symbols(&self) -> Vec<(Reg, String)> {
        if self.active_fiber.is_some() {
            return Vec::new();
        }
        let Some(frame) = self
            .frame_at(0)
            .filter(|frame| frame.function_id().is_none())
        else {
            return Vec::new();
        };
        frame
            .function()
            .locals
            .iter()
            .enumerate()
            .filter_map(|(index, name)| {
                let name = frame.unit().names.get(name.index())?.clone();
                Some((Reg(index as u16), name))
            })
            .chain(
                frame
                    .extras
                    .dynamic
                    .iter()
                    .map(|(name, register)| (*register, name.clone())),
            )
            .collect()
    }
}
