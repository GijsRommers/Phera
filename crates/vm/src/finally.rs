//! Finally bodies are heap-frame subroutines with typed continuations.
//!
//! For `try { return 7; } finally { echo "done"; }`, the compiler emits:
//!
//! ```text
//! LoadInt result, 7; CheckReturn result
//! FinallyCall state, cleanup, Return; ReturnChecked result
//! cleanup: Echo text; FinallyReturn state
//! ```
//!
//! Normal completion uses FinallyCall(Fallthrough) and resumes after the body.
//! Return/break/continue save their next instruction, which performs the exit
//! after all enclosing finally calls. An exception instead saves
//! Continuation::Throw; FinallyReturn restores that object to the unwind loop.
//! A return in cleanup emits FinallyDiscard and replaces either saved outcome.
//! The compiler reserves a state register for each lexical body; the VM stores
//! entered continuations in one lazy vector keyed by frame base and that register.
//! Ordinary PHP frames and calls never inspect or allocate continuation storage.

use crate::{Vm, exceptions::Thrown};
use phera_bytecode::{FinallyEntry, Instruction, Reg, Target};
use phera_extension_api::NativeError;
use phera_value::Object;
use std::rc::Rc;

/// The register identifies the compiler-reserved continuation slot, not a PHP value.
pub(crate) struct SavedFinally {
    /// Register windows identify active frames without touching ordinary frame layout.
    pub(crate) frame_base: usize,
    pub(crate) state: Reg,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) continuation: Continuation,
}

/// PHP distinguishes a saved structured exit from a pending thrown object.
pub(crate) enum Continuation {
    Resume { target: usize, reason: FinallyEntry },
    Throw { object: Rc<Object> },
}

impl Vm {
    /// Entering a try itself is free; only actual execution of finally saves state.
    pub(crate) fn enter_finally(
        &mut self,
        state: Reg,
        target: Target,
        continuation: Continuation,
    ) -> Result<(), NativeError> {
        let frame = self
            .call_stack
            .current_mut()
            .ok_or_else(|| internal_error!("finally without frame"))?;
        let end = frame.function(&self.code).instructions.iter().enumerate().skip(target.index())
            .find_map(|(index, instruction)| matches!(instruction, Instruction::FinallyReturn { state: found } if *found == state).then_some(index))
            .ok_or_else(|| internal_error!("finally without return"))?;
        self.finally_stack.push(SavedFinally {
            frame_base: frame.base,
            state,
            start: target.index(),
            end,
            continuation,
        });
        frame.ip = target.index();
        Ok(())
    }

    /// Remove the continuation before rethrowing so it cannot chain to itself.
    pub(crate) fn leave_finally(&mut self, state: Reg) -> Result<(), NativeError> {
        let continuation = self
            .take_finally(state)
            .ok_or_else(|| internal_error!("missing finally continuation"))?;
        match continuation {
            Continuation::Resume { target, reason } => {
                // The reason documents the pending action; the compiler emitted that
                // action at target, including all remaining outer finally calls.
                let _ = reason;
                self.jump(Target(target as u32))
            }
            Continuation::Throw { object } => Err(Thrown {
                object,
                skip_handlers: None,
            }
            .into_error()),
        }
    }

    /// A return or loop exit from finally takes precedence over its saved outcome.
    pub(crate) fn discard_finally(&mut self, state: Reg) {
        self.take_finally(state);
    }

    /// Slots are removed after every path, including repeated executions in a loop.
    fn take_finally(&mut self, state: Reg) -> Option<Continuation> {
        let frame_base = self.current()?.base;
        let index = self
            .finally_stack
            .iter()
            .rposition(|entry| entry.frame_base == frame_base && entry.state == state)?;
        Some(self.finally_stack.remove(index).continuation)
    }
}
