//! A fiber's `global $counter` sees the request symbol while file scope is parked.

use crate::Vm;

impl Vm {
    pub(super) fn export_main_globals(&mut self) {
        if self.active_fiber.is_none() {
            self.park_global_registers();
        }
    }

    pub(super) fn import_main_globals(&mut self) {
        if self.active_fiber.is_none() {
            self.restore_global_registers();
        }
    }
}
