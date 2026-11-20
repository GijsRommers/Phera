//! The interpreter loop: simple instructions, direct calls and plain returns
//! on borrowed code and registers.
//!
//! ```php
//! function fib($n) { if ($n < 2) return $n; return fib($n - 1) + fib($n - 2); }
//! ```
//!
//! Every instruction of `fib` runs here without leaving the loop: the
//! comparison and arithmetic in `fast::scalar_step`, the two calls in
//! [`enter_direct`], the returns in [`leave_plain`]. The loop borrows the code
//! table, the call stack and the registers, three separate fields of the VM,
//! so it needs no owner of the code and no `&mut Vm`. Anything that needs
//! more (a warning, a method, an exception) stops the loop with
//! [`LoopExit::Slow`]; the VM runs that one instruction through
//! `Vm::execute` and starts the loop again where the frame now is.

use phera_bytecode::{Instruction, Reg};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;
use crate::calls::call_stack::CallStack;
use crate::calls::frame::{Frame, TraversalMode};
use crate::calls::registers::Registers;
use crate::classes::ClassTable;
use crate::code_table::CodeTable;
use crate::fast::{ScalarStep, array_index_step, scalar_step};

/// How many loop rounds or calls a script may run before it lets other tasks
/// on the same thread have a turn.
///
/// Handing over is not free: the scheduler asks the operating system whether
/// any socket or timer is ready, which is a system call. At 4,096 that cost
/// 15 percent of a plain loop's time. 65,536 loop rounds are about a
/// millisecond, which is still far shorter than anyone waiting for a response
/// can notice.
pub(crate) const YIELD_AFTER_STEPS: usize = 65_536;

/// Why the async shell got control back.
pub(crate) enum BatchExit {
    /// An instruction that must be awaited (an include, a checked return).
    Instruction(Instruction),
    /// A prepared call to a native function, whose result goes to this register.
    NativeCall(Reg),
    /// A synchronous native call needs a PHP property getter to finish.
    NativeView(Reg, Box<NativeError>),
    /// A synchronous reflection method needs a PHP hook or lazy initializer to finish.
    NativeMethodHook(Reg, Box<(NativeError, std::rc::Rc<crate::classes::Method>)>),
    /// A prepared PHP call whose weak string argument may invoke __toString.
    BindCall(Reg),
    /// A comparison reached an object whose __toString must run.
    Compare(crate::string_comparison::PendingComparison),
    /// The frame that native code called has returned this value.
    Returned(Value),
    /// A PHP yield published a suspension for the generator driver.
    GeneratorSuspended,
    /// The work budget is spent; other tasks get a turn.
    Yield,
    /// Output or diagnostics queued a callback, which must run now.
    Handlers,
    Destructors,
    /// The first generator changed which deferred work this request can queue.
    GeneratorTracking,
}

/// Why the loop stopped. Small enough to be returned in registers.
enum LoopExit {
    /// This instruction needs the VM; the frame's position is published.
    Slow(Instruction),
    Yield,
    /// A call or return released an object whose destructor is due; the
    /// frame that runs next is current, with its position stored.
    Destructors,
    /// The instruction pointer left the function's code.
    Invalid,
}

/// The parts of the VM the loop borrows, and its counters.
pub(super) struct Machine<'a> {
    pub(super) code: &'a CodeTable,
    pub(super) classes: &'a mut ClassTable,
    pub(super) stack: &'a mut CallStack,
    pub(super) registers: &'a mut Registers,
    /// Loop rounds and calls since the last hand-over.
    work: usize,
    /// Instructions run, for `Vm::executed_instructions`.
    executed: u64,
    /// The depth at which the function that native code called returns.
    pub(super) stop: usize,
    pub(super) frame_limit: usize,
    /// The request's object ids, which say whether a destructor is due.
    pub(super) ids: &'a phera_value::ObjectIds,
}

/// Where the loop is: which function, its registers, its next instruction.
#[derive(Clone, Copy)]
pub(super) struct Position {
    pub(super) frame: Frame,
    pub(super) ip: usize,
}

impl Vm {
    /// Run frames until an instruction needs the async shell, the work budget
    /// is spent, or the frame at depth `stop` has returned.
    pub(crate) fn run_batch(
        &mut self,
        stop: usize,
        work: &mut usize,
    ) -> Result<BatchExit, NativeError> {
        let exit = if self.generator_releases.is_some() {
            self.run_batch_loop::<true>(stop, work)
        } else {
            self.run_batch_loop::<false>(stop, work)
        }?;
        if matches!(exit, BatchExit::Yield) || *work >= YIELD_AFTER_STEPS {
            *work = 0;
            self.check_execution_time()?;
        }
        Ok(exit)
    }

    #[inline(never)]
    fn run_batch_loop<const TRACK_GENERATORS: bool>(
        &mut self,
        stop: usize,
        work: &mut usize,
    ) -> Result<BatchExit, NativeError> {
        loop {
            let mut machine = Machine {
                code: &self.code,
                classes: &mut self.classes,
                stack: &mut self.call_stack,
                registers: &mut self.registers,
                work: *work,
                executed: 0,
                stop,
                frame_limit: self.frame_limit,
                ids: &self.object_ids,
            };
            // While no live object has a destructor due, no release can
            // make one due, and the loop is the plain one (see
            // `crate::destructors`). Making such an object leaves the loop,
            // so the next batch is guarded.
            let exit = if self.object_ids.has_due() {
                machine.run_guarded()
            } else {
                machine.run::<false>()
            };
            *work = machine.work;
            self.executed_instructions += machine.executed;
            let instruction = match exit {
                LoopExit::Slow(instruction) => instruction,
                LoopExit::Yield => return Ok(BatchExit::Yield),
                LoopExit::Destructors => return Ok(BatchExit::Destructors),
                LoopExit::Invalid => return Err(internal_error!("invalid IP")),
            };
            if let Some(exit) = self.execute::<TRACK_GENERATORS>(instruction, stop, work)? {
                return Ok(exit);
            }
            if self.destructors_due() {
                return Ok(BatchExit::Destructors);
            }
            if self.pending_callbacks || (TRACK_GENERATORS && self.generator_releases_pending()) {
                return Ok(BatchExit::Handlers);
            }
            if *work >= YIELD_AFTER_STEPS {
                *work = 0;
                return Ok(BatchExit::Yield);
            }
        }
    }
}

impl Machine<'_> {
    /// The loop while some object has a destructor due. Kept out of line,
    /// so that it does not change how the plain loop compiles.
    #[cold]
    #[inline(never)]
    fn run_guarded(&mut self) -> LoopExit {
        self.run::<true>()
    }

    /// The loop itself. Simple instructions run in [`Machine::run_simple`];
    /// this loop takes the others and switches frames on calls and returns.
    fn run<const GUARDED: bool>(&mut self) -> LoopExit {
        let [.., frame] = self.stack.frames() else {
            return LoopExit::Invalid;
        };
        let frame = *frame;
        let mut position = Position {
            frame,
            ip: frame.ip(),
        };
        let mut instructions = self.code.instructions(frame.function);
        loop {
            let instruction = match self.run_simple::<GUARDED>(instructions, &mut position) {
                Ok(instruction) => instruction,
                Err(exit) => return exit,
            };
            match self.step::<GUARDED>(instruction, position) {
                Step::Done(ip) => position.ip = ip,
                Step::Switched(next) => {
                    // A return releases the frame's variables. Simple
                    // instructions release nothing (see `crate::fast`), and
                    // other steps write registers that hold nothing.
                    if GUARDED && self.ids.has_doomed() {
                        return destructors_due();
                    }
                    position = next;
                    instructions = self.code.instructions(next.frame.function);
                }
                Step::Slow => {
                    self.publish(position.ip);
                    return LoopExit::Slow(instruction);
                }
                Step::Yield => return LoopExit::Yield,
            }
        }
    }

    /// Run simple instructions of one frame with its registers borrowed and
    /// the counters in locals, until one is not simple. Returns that one,
    /// with `position` just past it.
    #[inline(always)]
    fn run_simple<const GUARDED: bool>(
        &mut self,
        instructions: &[Instruction],
        position: &mut Position,
    ) -> Result<Instruction, LoopExit> {
        let slots = self.registers.tail_mut(position.frame.base());
        let mut ip = position.ip;
        let mut executed = 0;
        let mut work = self.work;
        let result = loop {
            let Some(instruction) = instructions.get(ip) else {
                break Err(LoopExit::Invalid);
            };
            let previous = ip;
            ip += 1;
            executed += 1;
            let step = match scalar_step::<GUARDED>(instruction, slots, &mut ip) {
                ScalarStep::Fallback => match *instruction {
                    Instruction::FetchIndex {
                        dst, base, index, ..
                    } => array_index_step::<GUARDED>(dst, base, index, slots),
                    _ => ScalarStep::Fallback,
                },
                complete => complete,
            };
            if step == ScalarStep::Fallback {
                break Ok(*instruction);
            }
            // Only backward jumps count as work; comparing positions also
            // covers fused compare-and-branch instructions.
            if ip <= previous {
                work += 1;
                if work >= YIELD_AFTER_STEPS {
                    work = 0;
                    break Err(LoopExit::Yield);
                }
            }
        };
        self.executed += executed;
        self.work = work;
        position.ip = ip;
        if result.is_err() {
            self.publish(ip);
        }
        result
    }

    /// The instructions that are not simple but need only the borrowed
    /// state: `foreach` over a snapshot, direct calls and plain returns.
    #[inline(always)]
    fn step<const GUARDED: bool>(&mut self, instruction: Instruction, position: Position) -> Step {
        let base = position.frame.base();
        let mut ip = position.ip;
        match instruction {
            Instruction::IterNext { iter, value, end } if position.frame.has_extras() => {
                let iterators = &mut self.stack.current_extras_mut().iterators;
                let slots = self.registers.tail_mut(base);
                match crate::iteration::snapshot_step(iter, value, end, iterators, slots, &mut ip) {
                    true => Step::Done(ip),
                    false => Step::Slow,
                }
            }
            Instruction::IterNextKeyed {
                iter, key, value, ..
            } if position.frame.has_extras() => {
                let iterators = &mut self.stack.current_extras_mut().iterators;
                let slots = self.registers.tail_mut(base);
                if iterators.get_mut(iter.0).is_some_and(|state| {
                    state.traversal_mode == TraversalMode::Value
                        && crate::iteration::native_step(state, Some(key), value, slots)
                }) {
                    Step::Done(ip)
                } else {
                    Step::Slow
                }
            }
            Instruction::FetchIndex {
                dst,
                base,
                index,
                mode: phera_bytecode::FetchMode::Read,
                ..
            } => match self.fetch_native_dimension(position, dst, base, index) {
                Step::Slow => self.further_step::<GUARDED>(instruction, position),
                done => done,
            },
            Instruction::AppendLocal { local, src } => {
                self.write_native_dimension(position, local, None, src)
            }
            Instruction::AssignPath { path, src } if self.classes.has_dimensions => {
                match self.write_dimension_path(position, path, src) {
                    Step::Slow => self.further_step::<GUARDED>(instruction, position),
                    done => done,
                }
            }
            Instruction::CallDirect {
                function,
                args,
                count,
                dst,
                ..
            } => self.enter_direct(position, function, (args, count), dst),
            Instruction::InitMethodCall { receiver, method } => {
                self.prepare_method(position, receiver, method)
            }
            Instruction::InitStaticCall {
                class,
                method,
                binding,
            } => self.prepare_static(position, (class, method), binding),
            Instruction::SendValue { src } => self.send(position, src),
            Instruction::SendLocal { local } => match self.send(position, local) {
                Step::Slow => self.send_reference(position, local),
                done => done,
            },
            Instruction::ReceiveDefault { param, passed } => {
                self.receive_default(position, param, passed)
            }
            Instruction::LoadConst { dst, constant } => self.load_constant(position, dst, constant),
            Instruction::CheckType { local, ty } => self.check_type(position, local, ty),
            Instruction::Call { dst, .. } => self.enter_prepared(position, dst),
            Instruction::New { class } => self.new_object::<GUARDED>(position, class),
            Instruction::InitDynamicCall { callee } => self.prepare_closure(position, callee),
            Instruction::FetchThis { dst } | Instruction::ThisOrNull { dst } => {
                self.fetch_this(position, dst)
            }
            Instruction::Return { src } => self.leave_plain::<GUARDED>(Some(src)),
            Instruction::ReturnNull => self.leave_plain::<GUARDED>(None),
            // Listed, so that the instructions further_step never takes do
            // not pay for a call to it on their way to the VM.
            Instruction::Binary { .. }
            | Instruction::Unary { .. }
            | Instruction::JumpIfNull { .. }
            | Instruction::JumpIfNotNull { .. }
            | Instruction::NewArray { .. }
            | Instruction::FetchIndex { .. }
            | Instruction::Fetch { .. }
            | Instruction::AssignPath { .. }
            | Instruction::IncDecPath { .. }
            | Instruction::InitCall { .. }
            | Instruction::FetchClassName { .. } => {
                self.further_step::<GUARDED>(instruction, position)
            }
            _ => Step::Slow,
        }
    }

    /// Count a loop round or a call; `true` when it is time to hand over.
    #[inline(always)]
    pub(super) fn spend_work(&mut self) -> bool {
        self.work += 1;
        if self.work >= YIELD_AFTER_STEPS {
            self.work = 0;
            return true;
        }
        false
    }

    /// Store the running frame's next instruction, before anything else
    /// looks at the frame.
    pub(super) fn publish(&mut self, ip: usize) {
        if let Some(frame) = self.stack.current_mut() {
            frame.set_ip(ip);
        }
    }
}

/// What a non-simple step did.
pub(super) enum Step {
    /// Finished; continue at this instruction.
    Done(usize),
    /// Entered or left a frame; continue there.
    Switched(Position),
    /// Needs the VM.
    Slow,
    /// A call spent the rest of the budget and published the next frame.
    Yield,
}

/// Leaves the loop for due destructors; out of line, since it is rare.
#[cold]
#[inline(never)]
fn destructors_due() -> LoopExit {
    LoopExit::Destructors
}
