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

use phera_bytecode::{FetchMode, Instruction, NameId, PathBase, PathId, Reg, SymbolId};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::fast::{ScalarStep, store};
use crate::fast_values::value_step;
use crate::unit_caches::GlobalBinding;

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
            | Instruction::BinaryInt { .. }
            | Instruction::BinaryBranch { .. }
            | Instruction::BinaryBranchIf { .. }
            | Instruction::BinaryIntBranch { .. }
            | Instruction::BinaryIntBranchIf { .. }
            | Instruction::AssignOp { .. }
            | Instruction::Unary { .. }
            | Instruction::Cast { .. }
            | Instruction::JumpIfNull { .. }
            | Instruction::JumpIfNotNull { .. }
            | Instruction::NewArray { .. }
            | Instruction::ArraySet { .. }
            | Instruction::ArrayPush { .. }
            | Instruction::ArrayKeyExists { .. }
            | Instruction::FetchIndex { .. } => self.value_step::<GUARDED>(&instruction, position),
            Instruction::InLiteralArray {
                dst,
                needle,
                haystack,
                strict,
            } => self.in_literal_array::<GUARDED>(position, dst, (needle, haystack, strict)),
            Instruction::CallNativeDirect {
                function,
                args,
                count,
                dst,
                ..
            } => self.call_native_direct::<GUARDED>(position, function, (args, count), dst),
            Instruction::Fetch {
                dst,
                path,
                mode: FetchMode::Read,
            } => match self.fetch_property::<GUARDED>(position, dst, path) {
                Step::Slow => match self.fetch_local_element::<GUARDED>(
                    position,
                    dst,
                    (path, FetchMode::Read),
                ) {
                    Step::Slow => {
                        self.fetch_static::<GUARDED>(position, dst, (path, FetchMode::Read))
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
                        match self.fetch_local_element::<GUARDED>(position, dst, (path, mode)) {
                            Step::Slow => self.fetch_static::<GUARDED>(position, dst, (path, mode)),
                            done => done,
                        }
                    }
                    done => done,
                },
            },
            // Declared property first (disjoint from `[Index]`/`[Append]` dimensions),
            // then ArrayAccess dimensions, then plain array element writes.
            Instruction::AssignPath { path, src } => {
                match self.assign_property::<GUARDED>(position, path, (src, None)) {
                    Step::Slow => {
                        let written = if self.classes.has_dimensions {
                            self.write_dimension_path(position, path, src)
                        } else {
                            Step::Slow
                        };
                        match written {
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
            Instruction::UnsetPath { path } => self.unset_element::<GUARDED>(position, path),
            Instruction::InitCall { function } => self.prepare_named_call(position, function),
            Instruction::FetchNamedClassConstant { dst, class, name } => {
                self.fetch_named_class_constant::<GUARDED>(position, dst, (class, name))
            }
            Instruction::FetchClassConstant { dst, class, name } => {
                self.fetch_class_constant::<GUARDED>(position, dst, (class, name))
            }
            Instruction::FetchConstant { dst, constant } => {
                self.fetch_known_constant::<GUARDED>(position, dst, constant)
            }
            Instruction::Assign { local, src } => {
                self.assign_through_reference::<GUARDED>(position, local, src)
            }
            Instruction::BindGlobal { local, name } => {
                self.bind_known_global::<GUARDED>(position, local, name)
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
            Instruction::InstanceOfBranch { src, class, target } => {
                self.instance_of_branch::<GUARDED>(position, src, class, target, false)
            }
            Instruction::InstanceOfBranchIf { src, class, target } => {
                self.instance_of_branch::<GUARDED>(position, src, class, target, true)
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

    /// `FetchConstant` of a scalar constant this site already read, while
    /// no constant has been defined since (see
    /// [`crate::unit_caches::CachedConstant`]).
    fn fetch_known_constant<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        constant: SymbolId,
    ) -> Step {
        let Some(value) = self
            .code
            .cache(position.frame.function)
            .known_constant(constant.index(), self.constant_epoch)
        else {
            return Step::Slow;
        };
        self.store_quiet::<GUARDED>(position, destination, value)
    }

    /// `global $name` of a variable this site bound before, while it is
    /// where it was then (the same register of the same file-scope code, or
    /// the same entry of the globals table) and already a PHP reference:
    /// the local becomes another handle on it. The first binding, which
    /// makes the reference, is the VM's.
    fn bind_known_global<const GUARDED: bool>(
        &mut self,
        position: Position,
        local: Reg,
        name: NameId,
    ) -> Step {
        let Some(binding) = self
            .code
            .cache(position.frame.function)
            .known_global(name.index())
        else {
            return Step::Slow;
        };
        let cell = match binding {
            GlobalBinding::Register { function, register } => {
                // The innermost of the file-scope frames at the bottom of
                // the stack holds the global variables (`Vm::global_frame`).
                let Some(file_scope) = self
                    .stack
                    .frames()
                    .iter()
                    .take_while(|frame| self.code.local(frame.function).is_none())
                    .last()
                else {
                    return Step::Slow;
                };
                if file_scope.function != function {
                    return Step::Slow;
                }
                let index = file_scope.base() + register.index();
                match self.registers.window(index, 1).first() {
                    Some(Some(Value::Reference(cell))) => cell,
                    _ => return Step::Slow,
                }
            }
            GlobalBinding::Table { index } => {
                let names = &self.code.cache(position.frame.function).names;
                let Some(name) = names.get(name.index()) else {
                    return Step::Slow;
                };
                match self.globals.reference_at(index as usize, name.as_bytes()) {
                    Some(cell) => cell,
                    None => return Step::Slow,
                }
            }
        };
        let value = Value::Reference(cell.clone());
        self.store_quiet::<GUARDED>(position, local, value)
    }

    /// `$global = $scalar` where the local is a PHP reference that holds a
    /// scalar or a string: the value goes into the shared cell, as the
    /// VM's `Assign` writes it. Nothing is released that could run PHP
    /// code, no typed property constrains the reference, and no `foreach`
    /// of the frame can be iterating it (the frame has no side table).
    fn assign_through_reference<const GUARDED: bool>(
        &mut self,
        position: Position,
        local: Reg,
        source: Reg,
    ) -> Step {
        if GUARDED || position.frame.has_extras() {
            return Step::Slow;
        }
        let slots = self.registers.tail_mut(position.frame.base());
        let Some(Some(Value::Reference(cell))) = slots.get(local.index()) else {
            return Step::Slow;
        };
        if !self.property_references.borrow().constraints.is_empty() {
            return Step::Slow;
        }
        let value = match slots.get(source.index()) {
            Some(Some(value)) if is_plain_scalar(value) => value.clone(),
            _ => return Step::Slow,
        };
        let Ok(mut target) = cell.try_borrow_mut() else {
            return Step::Slow;
        };
        if !is_plain_scalar(&target) {
            return Step::Slow;
        }
        *target = value;
        Step::Done(position.ip)
    }

    /// Stores what a quiet read found.
    pub(super) fn store_quiet<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        value: Value,
    ) -> Step {
        let slots = self.registers.tail_mut(position.frame.base());
        match store::<GUARDED, _>(slots, destination, value) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
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
            ScalarStep::Complete | ScalarStep::Released => Step::Done(ip),
        }
    }
}

/// A value whose release never runs PHP code: no array, object or resource.
fn is_plain_scalar(value: &Value) -> bool {
    matches!(
        value,
        Value::Null
            | Value::Bool(_)
            | Value::Int(_)
            | Value::Float(_)
            | Value::InternedStr(_)
            | Value::String(_)
    )
}
