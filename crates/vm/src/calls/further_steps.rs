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
//! writes (`fast_properties`), calls by name (`fast_named_calls`), class
//! names (`fast_class_names`), `instanceof` (`fast_instance_of`), `foreach` start and end
//! (`fast_foreach`). They
//! are reached through this one function, kept out of line, because every
//! case added to `step` itself changes how the compiler lays out the
//! whole loop (measured: two to five percent on loops that never run
//! them). Each answers [`Step::Slow`], having changed nothing, when its
//! case is not the plain one, and the VM runs the instruction.

use phera_bytecode::{FetchMode, Instruction, PathBase, PathId, Reg};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::fast::{ScalarStep, store};
use crate::fast_values::value_step;

impl Machine<'_> {
    /// Runs `instruction` if it is one of the cases above. Marked cold so
    /// that the loop's own code is laid out as if this branch were rare,
    /// which keeps tight loops that never take it as they were.
    #[cold]
    #[inline(never)]
    pub(super) fn further_step<const GUARDED: bool>(
        &mut self,
        instruction: Instruction,
        position: Position,
    ) -> Step {
        match instruction {
            // An object with ArrayAccess first, then an array.
            Instruction::FetchIndex {
                dst,
                base,
                index,
                mode: FetchMode::Read,
                ..
            } => match self.fetch_native_dimension(position, dst, base, index) {
                Step::Slow => self.value_step::<GUARDED>(&instruction, position),
                done => done,
            },
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
            } => match self.fetch_property::<GUARDED>(position, dst, path) {
                Step::Slow => match self.fetch_property_element::<GUARDED>(
                    position,
                    dst,
                    (path, FetchMode::Read),
                ) {
                    Step::Slow => {
                        self.fetch_local_element::<GUARDED>(position, dst, (path, FetchMode::Read))
                    }
                    done => done,
                },
                done => done,
            },
            Instruction::Fetch {
                dst,
                path,
                mode: FetchMode::Destructure,
            } => self.fetch_local_element::<GUARDED>(position, dst, (path, FetchMode::Destructure)),
            Instruction::Fetch {
                dst,
                path,
                mode: mode @ (FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty),
            } => match self.quiet_variable(path, position) {
                Some(found) => self.store_quiet::<GUARDED>(position, dst, found),
                None => match self.fetch_property_quietly::<GUARDED>(position, dst, path) {
                    Step::Slow => {
                        match self.fetch_property_element::<GUARDED>(position, dst, (path, mode)) {
                            Step::Slow => {
                                self.fetch_local_element::<GUARDED>(position, dst, (path, mode))
                            }
                            done => done,
                        }
                    }
                    done => done,
                },
            },
            // An element of an object with ArrayAccess, then a property.
            Instruction::AssignPath { path, src } => {
                let written = if self.classes.has_dimensions {
                    self.write_dimension_path(position, path, src)
                } else {
                    Step::Slow
                };
                match written {
                    Step::Slow => {
                        match self.assign_property::<GUARDED>(position, path, (src, None)) {
                            Step::Slow => {
                                self.assign_element::<GUARDED>(position, path, (src, None))
                            }
                            done => done,
                        }
                    }
                    done => done,
                }
            }
            // Only arrays are written here, so an `ArrayAccess` object on
            // the path is left to the VM as well.
            Instruction::AssignPathFromLocal { path, src, dst } => {
                match self.assign_property::<GUARDED>(position, path, (src, Some(dst))) {
                    Step::Slow => self.assign_element::<GUARDED>(position, path, (src, Some(dst))),
                    done => done,
                }
            }
            Instruction::AssignOpPath { op, dst, path, src } => {
                self.assign_operation::<GUARDED>(position, (op, dst), path, src)
            }
            Instruction::IncDecPath { mode, dst, path } => {
                match self.increment_property::<GUARDED>(position, (mode, dst), path) {
                    Step::Slow => self.increment_element::<GUARDED>(position, (mode, dst), path),
                    done => done,
                }
            }
            Instruction::InitCall { function } => self.prepare_named_call(position, function),
            Instruction::FetchNamedClassConstant { dst, class, name } => {
                self.fetch_named_class_constant::<GUARDED>(position, dst, (class, name))
            }
            Instruction::FetchClassName { dst, class } => {
                self.fetch_class_name::<GUARDED>(position, dst, class)
            }
            Instruction::SendPath { path } => self.send_path_value(position, path),
            Instruction::IterStart { iter, src, .. } => {
                self.start_array_iteration::<GUARDED>(position, iter, src)
            }
            Instruction::IterEnd { iter } => self.end_array_iteration::<GUARDED>(position, iter),
            Instruction::InstanceOf { dst, src, class } => {
                self.instance_of::<GUARDED>(position, dst, src, class)
            }
            _ => Step::Slow,
        }
    }

    /// A quiet read of a plain variable (`$value ?? $default`): its value,
    /// or null without a warning when it is unset. `None` for any other
    /// path, a variable that is a PHP reference, or code whose variables
    /// can also be reached by name (top-level code, `extract`, `$$name`),
    /// where an empty register does not prove the variable unset.
    fn quiet_variable(&self, path: PathId, position: Position) -> Option<Value> {
        let function = self.code.function(position.frame.function);
        if function.dynamic_scope {
            return None;
        }
        let path = function.paths.get(path.index())?;
        let (PathBase::Register(register), []) = (path.base, path.steps.as_slice()) else {
            return None;
        };
        match self
            .registers
            .window(position.frame.base() + register.index(), 1)
        {
            [Some(Value::Reference(_))] | [] => None,
            [Some(value)] => Some(value.clone()),
            [None] => Some(Value::Null),
            _ => None,
        }
    }

    /// Stores what a quiet read found.
    pub(super) fn store_quiet<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        value: Value,
    ) -> Step {
        let slots = self.registers.tail_mut(position.frame.base());
        match store::<GUARDED>(slots, destination, value) {
            ScalarStep::Complete => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
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
