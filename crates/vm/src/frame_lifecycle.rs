//! Allocate and recycle PHP heap frames without recursive Rust calls.

use std::sync::Arc;

use phera_bytecode::{CompiledUnit, Reg};

use crate::{SymbolCache, Vm, dynamic_variables, frame::Frame};

impl Vm {
    /// Reuse frame-owned call buffers after return. Recursion grows by depth,
    /// rather than allocating fresh preparation buffers for every invocation.
    pub(crate) fn make_frame(
        &mut self,
        unit: &Arc<CompiledUnit>,
        function_id: Option<usize>,
        destination: Option<Reg>,
    ) -> Box<Frame> {
        if let Some(mut frame) = self.call_stack.take_recycled() {
            frame.method = None;
            // Mirrors replacing the unit/cache owners, without atomic Arc
            // increments when a recycled frame already owns this exact unit.
            if !Arc::ptr_eq(&frame.unit, unit) {
                frame.cache = self.code_cache(unit);
                frame.unit = unit.clone();
            }
            frame.function_id = function_id;
            frame.destination = destination;
            frame.ip = 0;
            frame.base = self.registers.len();
            frame.count = frame.function().registers as usize;
            frame.arguments.clear();
            frame.calls.clear();
            frame.dynamic =
                dynamic_variables::DynamicVariables::new(frame.function().dynamic_scope.into());
            frame.iterators.resize(frame.function().iterators);
            self.registers.extend_to(frame.base + frame.count);
            frame
        } else {
            let cache = self.code_cache(unit);
            Box::new(Frame::new(
                cache,
                &mut self.registers,
                unit.clone(),
                function_id,
                destination,
            ))
        }
    }

    /// Share resolved call symbols across activations of the same compiled unit.
    pub(crate) fn code_cache(&mut self, unit: &Arc<CompiledUnit>) -> SymbolCache {
        self.unit_caches.symbols(unit)
    }
}
