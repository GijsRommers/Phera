//! A timeout is a fatal error: as after any fatal error, the objects alive
//! then get no destructor, and those that shutdown callbacks create do (see
//! `crate::request_end`). Here the globals stay visible to the shutdown
//! callbacks although the main script's frame was abandoned.

use crate::Vm;

impl Vm {
    pub(crate) fn mark_timed_out_objects(&mut self) {
        self.timed_out_objects = Some(self.classes.objects.clone());
    }

    /// Globals outlive the abandoned main frame and remain visible in shutdown.
    pub(crate) fn retain_timeout_globals(&mut self) {
        let Some(main) = self
            .frame_at(0)
            .filter(|frame| frame.function_id().is_none())
        else {
            return;
        };
        let slots = main.slots(&self.registers);
        let named = main
            .function()
            .locals
            .iter()
            .enumerate()
            .filter_map(|(index, name)| {
                main.unit()
                    .names
                    .get(name.index())
                    .map(|name| (name, index))
            });
        let dynamic = main
            .extras
            .dynamic
            .iter()
            .map(|(name, register)| (name, register.index()));
        let values: Vec<_> = named
            .chain(dynamic)
            .map(|(name, index)| (name.clone(), slots.get(index).cloned().flatten()))
            .collect();
        for (name, value) in values {
            if let Some(value) = value {
                self.globals.insert(name, value);
            } else {
                self.globals.remove(&name);
            }
        }
    }
}
