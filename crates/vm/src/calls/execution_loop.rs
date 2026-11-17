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

use phera_bytecode::{FunctionId, Instruction, Reg};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;
use crate::calls::call_stack::CallStack;
use crate::calls::frame::Frame;
use crate::calls::registers::Registers;
use crate::code_table::{CodeTable, FunctionShape};
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
    /// The frame that native code called has returned this value.
    Returned(Value),
    /// The work budget is spent; other tasks get a turn.
    Yield,
    /// A warning queued a user error handler, which must run now.
    Handlers,
}

/// Why the loop stopped. Small enough to be returned in registers.
enum LoopExit {
    /// This instruction needs the VM; the frame's position is published.
    Slow(Instruction),
    Yield,
    /// The instruction pointer left the function's code.
    Invalid,
}

/// The parts of the VM the loop borrows, and its counters.
struct Machine<'a> {
    code: &'a CodeTable,
    stack: &'a mut CallStack,
    registers: &'a mut Registers,
    /// Loop rounds and calls since the last hand-over.
    work: usize,
    /// Instructions run, for `Vm::executed_instructions`.
    executed: u64,
    /// The depth at which the function that native code called returns.
    stop: usize,
    frame_limit: usize,
}

/// Where the loop is: which function, its registers, its next instruction.
#[derive(Clone, Copy)]
struct Position {
    frame: Frame,
    ip: usize,
}

impl Vm {
    /// Run frames until an instruction needs the async shell, the work budget
    /// is spent, or the frame at depth `stop` has returned.
    pub(crate) fn run_batch(
        &mut self,
        stop: usize,
        work: &mut usize,
    ) -> Result<BatchExit, NativeError> {
        loop {
            let mut machine = Machine {
                code: &self.code,
                stack: &mut self.call_stack,
                registers: &mut self.registers,
                work: *work,
                executed: 0,
                stop,
                frame_limit: self.frame_limit,
            };
            let exit = machine.run();
            *work = machine.work;
            self.executed_instructions += machine.executed;
            let instruction = match exit {
                LoopExit::Slow(instruction) => instruction,
                LoopExit::Yield => return Ok(BatchExit::Yield),
                LoopExit::Invalid => return Err(internal_error!("invalid IP")),
            };
            if let Some(exit) = self.execute(instruction, stop, work)? {
                return Ok(exit);
            }
            if !self.error_handlers.pending.is_empty() {
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
    /// The loop itself. Simple instructions run in [`Machine::run_simple`];
    /// this loop takes the others and switches frames on calls and returns.
    fn run(&mut self) -> LoopExit {
        let Some(frame) = self.stack.current().copied() else {
            return LoopExit::Invalid;
        };
        let mut position = Position {
            frame,
            ip: frame.ip(),
        };
        loop {
            let instructions = self.code.instructions(position.frame.function);
            let instruction = match self.run_simple(instructions, &mut position) {
                Ok(instruction) => instruction,
                Err(exit) => return exit,
            };
            match self.step(instruction, position) {
                Step::Done(ip) => position.ip = ip,
                Step::Switched(next) => position = next,
                Step::Slow => {
                    self.publish(position.ip);
                    return LoopExit::Slow(instruction);
                }
            }
        }
    }

    /// Run simple instructions of one frame with its registers borrowed and
    /// the counters in locals, until one is not simple. Returns that one,
    /// with `position` just past it.
    #[inline(always)]
    fn run_simple(
        &mut self,
        instructions: &[Instruction],
        position: &mut Position,
    ) -> Result<Instruction, LoopExit> {
        let slots = self.registers.tail_mut(position.frame.base());
        let mut ip = position.ip;
        let mut executed = 0;
        let mut work = self.work;
        let result = loop {
            let Some(&instruction) = instructions.get(ip) else {
                break Err(LoopExit::Invalid);
            };
            let previous = ip;
            ip += 1;
            executed += 1;
            let step = match scalar_step(instruction, slots, &mut ip) {
                ScalarStep::Fallback => match instruction {
                    Instruction::FetchIndex {
                        dst, base, index, ..
                    } => array_index_step(dst, base, index, slots),
                    _ => ScalarStep::Fallback,
                },
                complete => complete,
            };
            if step == ScalarStep::Fallback {
                break Ok(instruction);
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
    fn step(&mut self, instruction: Instruction, position: Position) -> Step {
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
            Instruction::CallDirect {
                function,
                args,
                count,
                dst,
            } => self.enter_direct(position, function, (args, count), dst),
            Instruction::Return { src } => self.leave_plain(Some(src)),
            Instruction::ReturnNull => self.leave_plain(None),
            _ => Step::Slow,
        }
    }

    /// Count a loop round or a call; `true` when it is time to hand over.
    #[inline(always)]
    fn spend_work(&mut self) -> bool {
        self.work += 1;
        if self.work >= YIELD_AFTER_STEPS {
            self.work = 0;
            return true;
        }
        false
    }

    /// Store the running frame's next instruction, before anything else
    /// looks at the frame.
    fn publish(&mut self, ip: usize) {
        if let Some(frame) = self.stack.current_mut() {
            frame.set_ip(ip);
        }
    }
}

/// What a non-simple step did.
enum Step {
    /// Finished; continue at this instruction.
    Done(usize),
    /// Entered or left a frame; continue there.
    Switched(Position),
    /// Needs the VM.
    Slow,
}

impl Machine<'_> {
    /// `CallDirect` to a plain function with valid arguments: move the
    /// arguments into a new window at the top of the register stack and
    /// push a frame. Anything unusual (too few arguments, an argument that
    /// fails its type's bit test, the frame limit) takes the general path.
    #[inline(always)]
    fn enter_direct(
        &mut self,
        caller: Position,
        declared: FunctionId,
        (args, count): (Reg, u16),
        destination: Reg,
    ) -> Step {
        let first = self.code.facts(caller.frame.function).unit_first;
        let Some((function, facts)) = self.code.declared_facts(first, declared.index()) else {
            return Step::Slow;
        };
        let count = count as usize;
        if facts.shape != FunctionShape::Plain
            || count < facts.required as usize
            || count > facts.parameters as usize
            || self.stack.depth() >= self.frame_limit
        {
            return Step::Slow;
        }
        let from = caller.frame.base() + args.index();
        let arguments = self.registers.window(from, count);
        let acceptable = arguments.iter().enumerate().all(|(index, argument)| {
            argument.as_ref().is_some_and(|value| {
                !value.is_reference()
                    && (!facts.is_typed(index)
                        || facts.parameter_masks[index].intersects(crate::types::own_type(value)))
            })
        });
        if !acceptable {
            return Step::Slow;
        }
        let base = self.registers.len();
        self.registers.grow_to(base + facts.registers as usize);
        for index in 0..count {
            self.registers.move_slot(from + index, base + index);
        }
        self.publish(caller.ip);
        let frame = Frame::new(function, base, Some(destination));
        self.stack.push(frame);
        if count < facts.parameters as usize {
            let arguments = &mut self.stack.current_extras_mut().arguments;
            arguments.supplied = Some(count);
            arguments.count = Some(count);
        }
        self.spend_work();
        Step::Switched(Position { frame, ip: 0 })
    }

    /// `Return` from a function frame that keeps nothing in the side table,
    /// returns to PHP code, and whose value passes its return type's bit test:
    /// write the value into the caller's register and empty the callee's.
    #[inline(always)]
    fn leave_plain(&mut self, source: Option<Reg>) -> Step {
        let Some(frame) = self.stack.current().copied() else {
            return Step::Slow;
        };
        let facts = self.code.facts(frame.function);
        let Some(destination) = frame.destination() else {
            return Step::Slow;
        };
        if frame.has_extras()
            || self.stack.depth() <= self.stop + 1
            || self.code.local(frame.function).is_none()
            || self.stack.has_calls_from(frame.base())
        {
            return Step::Slow;
        }
        let Some(caller) = self.stack.caller().copied() else {
            return Step::Slow;
        };
        let value = match source {
            Some(source) => match self.registers.window(frame.base() + source.index(), 1) {
                [Some(value)] if !value.is_reference() => value,
                _ => return Step::Slow,
            },
            None => &Value::Null,
        };
        if let Some(mask) = facts.return_mask
            && !mask.intersects(crate::types::own_type(value))
        {
            return Step::Slow;
        }
        let value = match source {
            Some(source) => self.registers.take(frame.base() + source.index()),
            None => Value::Null,
        };
        self.stack.pop();
        self.registers.truncate(frame.base());
        self.registers
            .put(caller.base() + destination.index(), value);
        Step::Switched(Position {
            frame: caller,
            ip: caller.ip(),
        })
    }
}
