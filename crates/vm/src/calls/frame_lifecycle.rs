//! Allocate and recycle PHP heap frames without recursive Rust calls.

use std::sync::Arc;

use phera_bytecode::{CompiledUnit, Reg};

use crate::{Vm, calls::frame::Frame};

impl Vm {
    /// Reuse activation storage; Init already reserved all registers for this call.
    pub(crate) fn make_frame(
        &mut self,
        unit: &Arc<CompiledUnit>,
        function_id: Option<usize>,
        destination: Option<Reg>,
        base: usize,
    ) -> Box<Frame> {
        if let Some(mut frame) = self.call_stack.take_recycled() {
            // Retained code ownership changes only when this slot switches compiled units.
            if !Arc::ptr_eq(&frame.unit, unit) {
                frame.cache = self.unit_caches.symbols(unit);
                frame.unit = unit.clone();
            }
            frame.function_id = function_id;
            frame.destination = destination;
            frame.ip = 0;
            frame.base = base;
            frame.count = frame.function().registers as usize;
            frame.saved_arguments = None;
            frame.extra_arguments = 0;
            frame.argument_count = 0;
            frame.call_start = self.call_stack.preparing.len();
            let scope = frame.function().dynamic_scope.into();
            frame.dynamic.reset(scope);
            frame.iterators.resize(frame.function().iterators);
            frame
        } else {
            let cache = self.unit_caches.symbols(unit);
            let mut frame = Box::new(Frame::new(
                cache,
                &mut self.registers,
                unit.clone(),
                function_id,
                destination,
                base,
            ));
            frame.call_start = self.call_stack.preparing.len();
            frame
        }
    }
}
