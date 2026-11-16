//! Allocate and recycle PHP heap frames without recursive Rust calls.

use std::sync::Arc;

use phera_bytecode::{CompiledUnit, Reg};

use crate::{SymbolCache, Vm, dynamic_variables, frame::Frame};

impl Vm {
    /// Reuse frame-owned call buffers after return. Recursion grows by depth,
    /// rather than allocating fresh preparation buffers for every invocation.
    pub(crate) fn make_frame(
        &mut self,
        unit: Arc<CompiledUnit>,
        function_id: Option<usize>,
        destination: Option<Reg>,
    ) -> Frame {
        let cache = if let Some(frame) = self
            .current()
            .filter(|frame| Arc::ptr_eq(&frame.unit, &unit))
        {
            frame.cache.clone()
        } else {
            self.code_cache(&unit)
        };
        if let Some(mut frame) = self.call_stack.take_recycled() {
            frame.unit = unit;
            frame.cache = cache;
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
            Frame::new(cache, &mut self.registers, unit, function_id, destination)
        }
    }

    /// Share resolved call symbols across activations of the same compiled unit.
    pub(crate) fn code_cache(&mut self, unit: &Arc<CompiledUnit>) -> SymbolCache {
        self.unit_caches.symbols(unit)
    }
}
