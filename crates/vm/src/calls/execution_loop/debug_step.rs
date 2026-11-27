//! A connected debugger publishes one guarded instruction at a time.
//!
//! `$this->routes = new RouteCollection;` must use the same constructor,
//! call, and register paths whether or not an IDE is attached.

use phera_bytecode::Instruction;
use phera_extension_api::NativeError;

use crate::Vm;
use crate::fast::{ScalarStep, array_index_step, scalar_step};

use super::{BatchExit, Machine, Position, Step, YIELD_AFTER_STEPS};

enum Progress {
    Advanced,
    Slow,
    Yield,
}

impl Vm {
    /// The request's debugger has already checked whether to stop at this instruction.
    #[cold]
    #[inline(never)]
    pub(crate) fn debug_instruction(
        &mut self,
        instruction: Instruction,
        stop: usize,
        work: &mut usize,
    ) -> Result<Option<BatchExit>, NativeError> {
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
            registry: &self.registry,
            class_states: &self.class_states,
            enum_cases: &self.enum_cases,
        };
        let progress = machine.debug_step(instruction)?;
        *work = machine.work;
        self.executed_instructions += 1;
        match progress {
            Progress::Advanced => Ok(None),
            Progress::Yield => {
                self.check_execution_time()?;
                Ok(Some(BatchExit::Yield))
            }
            Progress::Slow if self.generator_releases.is_some() => {
                self.execute::<true>(instruction, stop, work)
            }
            Progress::Slow => self.execute::<false>(instruction, stop, work),
        }
    }
}

impl Machine<'_> {
    fn debug_step(&mut self, instruction: Instruction) -> Result<Progress, NativeError> {
        let [.., frame] = self.stack.frames() else {
            return Err(internal_error!("missing debug frame"));
        };
        let frame = *frame;
        let previous = frame.ip();
        let mut ip = previous + 1;
        let slots = self.registers.tail_mut(frame.base());
        let scalar = match scalar_step::<true, _>(&instruction, slots, &mut ip) {
            ScalarStep::Fallback => match instruction {
                Instruction::FetchIndex {
                    dst,
                    base,
                    index,
                    mode,
                    ..
                } => array_index_step::<true, _>(dst, base, index, mode, slots),
                _ => ScalarStep::Fallback,
            },
            complete => complete,
        };
        if scalar != ScalarStep::Fallback {
            self.publish(ip);
            if ip <= previous {
                self.work += 1;
                if self.work >= YIELD_AFTER_STEPS {
                    self.work = 0;
                    return Ok(Progress::Yield);
                }
            }
            return Ok(Progress::Advanced);
        }
        Ok(
            match self.step::<true>(&instruction, Position { frame, ip }) {
                Step::Done(ip) => {
                    self.publish(ip);
                    Progress::Advanced
                }
                Step::Switched(next) => {
                    self.publish(next.ip);
                    Progress::Advanced
                }
                Step::Slow => {
                    self.publish(ip);
                    Progress::Slow
                }
                Step::Yield => Progress::Yield,
            },
        )
    }
}
