//! Entering `f(1)` copies numeric code and window addresses into a vector frame.

use crate::{Vm, calls::frame::Frame};
use phera_bytecode::{CompiledUnit, Reg};
use std::sync::Arc;

impl Vm {
    /// Create an activation from request-owned code; optional storage is indexed by depth.
    pub(crate) fn make_frame(
        &mut self,
        unit: &Arc<CompiledUnit>,
        function_id: Option<usize>,
        destination: Option<Reg>,
        base: usize,
    ) -> Frame {
        let function = self.code.index(unit, function_id);
        let frame = self.make_indexed_frame(function, destination, base);
        self.registers.grow_to(base + frame.count);
        frame
    }

    /// Ordinary calls already have a function address and never revisit unit loading.
    pub(crate) fn make_indexed_frame(
        &mut self,
        function: crate::code_table::FunctionIndex,
        destination: Option<Reg>,
        base: usize,
    ) -> Frame {
        let depth = self.call_stack.depth();
        if self.call_stack.extras.len() <= depth {
            self.call_stack
                .extras
                .push(super::frame_extras::FrameExtras::new());
        }
        let count = self.code.function(function).registers as usize;
        let iterators = self.code.function(function).iterators;
        if iterators > 0 {
            self.call_stack.extras[depth].iterators.resize(iterators);
        }
        Frame {
            function,
            depth,
            destination,
            base,
            count,
            ip: 0,
            call_start: self.call_stack.preparing.len(),
        }
    }
}
