//! The instructions the interpreter loop runs out of line.
//!
//! ```php
//! if ($user->name === 'admin') { ... }   // Fetch $user->name, Binary ===
//! $parts[] = trim($line);                 // InitCall of a linked function
//! ```
//!
//! `Machine::step` takes the instructions that need only the loop's
//! borrowed state and that are as frequent as calls and returns. The ones
//! here are frequent in framework code but rarer in tight loops: plain
//! value operators (`crate::fast_values`), declared property reads and
//! writes (`fast_properties`), calls by name (`fast_named_calls`). They
//! are reached through this one function, kept out of line, because every
//! case added to `step` itself changes how the compiler lays out the
//! whole loop (measured: two to five percent on loops that never run
//! them). Each answers [`Step::Slow`], having changed nothing, when its
//! case is not the plain one, and the VM runs the instruction.

use phera_bytecode::{FetchMode, Instruction};

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::fast::ScalarStep;
use crate::fast_values::value_step;

impl Machine<'_> {
    /// Runs `instruction` if it is one of the cases above.
    #[inline(never)]
    pub(super) fn further_step<const GUARDED: bool>(
        &mut self,
        instruction: Instruction,
        position: Position,
    ) -> Step {
        match instruction {
            Instruction::Binary { .. }
            | Instruction::Unary { .. }
            | Instruction::JumpIfNull { .. }
            | Instruction::JumpIfNotNull { .. }
            | Instruction::NewArray { .. }
            | Instruction::FetchIndex { .. } => self.value_step::<GUARDED>(&instruction, position),
            Instruction::Fetch {
                dst,
                path,
                mode: FetchMode::Read,
            } => self.fetch_property::<GUARDED>(position, dst, path),
            Instruction::AssignPath { path, src } => {
                self.assign_property::<GUARDED>(position, path, src)
            }
            Instruction::IncDecPath { mode, dst, path } => {
                self.increment_property::<GUARDED>(position, (mode, dst), path)
            }
            Instruction::InitCall { function } => self.prepare_named_call(position, function),
            _ => Step::Slow,
        }
    }

    /// A simple instruction on values other than integers, on the running
    /// frame's registers.
    fn value_step<const GUARDED: bool>(
        &mut self,
        instruction: &Instruction,
        position: Position,
    ) -> Step {
        let slots = self.registers.tail_mut(position.frame.base());
        let mut ip = position.ip;
        match value_step::<GUARDED>(instruction, slots, &mut ip) {
            ScalarStep::Fallback => Step::Slow,
            // A jump back is a loop round, like one in the inner loop.
            ScalarStep::Complete if ip < position.ip && self.spend_work() => {
                self.publish(ip);
                Step::Yield
            }
            ScalarStep::Complete => Step::Done(ip),
        }
    }
}
