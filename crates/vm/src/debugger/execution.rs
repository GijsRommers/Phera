//! Source stops are checked only in the batch that runs after a debugger connects.
//!
//! `step_over` ignores deeper PHP frames. The batch publishes the next
//! instruction before giving the request task back to the DBGp command loop.

use phera_bytecode::Instruction;
use phera_extension_api::NativeError;
use quick_xml::escape::escape;

use crate::Vm;
use crate::calls::execution_loop::{BatchExit, YIELD_AFTER_STEPS};
use crate::code_table::FunctionIndex;

use super::breakpoints::Kind;
use super::source::Sources;

#[derive(Clone, Copy)]
pub(super) struct Position {
    pub(super) depth: usize,
    base: usize,
    function: FunctionIndex,
    ip: usize,
    pub(super) line: u32,
}

impl Position {
    fn same_frame(self, previous: Self) -> bool {
        self.depth == previous.depth
            && self.base == previous.base
            && self.function == previous.function
    }
}

impl Vm {
    #[cold]
    #[inline(never)]
    pub(crate) fn debug_batch(
        &mut self,
        stop: usize,
        work: &mut usize,
    ) -> Result<Option<BatchExit>, NativeError> {
        loop {
            let frame = self
                .current()
                .ok_or_else(|| internal_error!("missing debug frame"))?;
            let ip = frame.ip();
            let function = frame.function();
            let instruction = *function
                .instructions
                .get(ip)
                .ok_or_else(|| internal_error!("invalid IP"))?;
            let position = Position {
                depth: self.call_stack.depth(),
                base: frame.base(),
                function: frame.frame.function,
                ip,
                line: function.lines.get(ip).copied().unwrap_or(1),
            };
            let mut debugger = self
                .debugger
                .take()
                .ok_or_else(|| internal_error!("missing debugger"))?;
            if self.debug_boundary(&mut debugger, position, instruction) {
                self.debugger = Some(debugger);
                return Ok(None);
            }
            debugger.last_exception = None;
            self.debugger = Some(debugger);
            if let Some(exit) = self.debug_instruction(instruction, stop, work)? {
                return Ok(Some(exit));
            }
            if self.destructors_due() {
                return Ok(Some(BatchExit::Destructors));
            }
            if self.pending_callbacks || self.generator_releases_pending() {
                return Ok(Some(BatchExit::Handlers));
            }
            if *work >= YIELD_AFTER_STEPS {
                *work = 0;
                self.check_execution_time()?;
                return Ok(Some(BatchExit::Yield));
            }
        }
    }

    fn debug_boundary(
        &self,
        debugger: &mut super::Session,
        position: Position,
        instruction: Instruction,
    ) -> bool {
        if debugger.poll_running() {
            return true;
        }
        if !is_breakable(instruction) && !debugger.force_break {
            return false;
        }
        let previous = debugger.visit(position);
        if !debugger.force_break
            && previous
                .is_some_and(|previous| position.same_frame(previous) && previous.ip == position.ip)
        {
            return false;
        }
        let entered = previous.is_none_or(|previous| !position.same_frame(previous));
        let line = entered
            || previous.is_some_and(|previous| {
                position.line != previous.line || position.ip < previous.ip
            });
        let returning = matches!(
            instruction,
            Instruction::Return { .. }
                | Instruction::ReturnNull
                | Instruction::ReturnChecked { .. }
                | Instruction::ReturnReference { .. }
                | Instruction::GeneratorReturn { .. }
        );
        if !line && !returning && !debugger.force_break {
            return false;
        }
        let Some(frame) = self.current() else {
            return false;
        };
        let file = debugger.sources.uri(frame.unit());
        if debugger.features.resolved_breakpoints && line {
            let resolved = debugger.breakpoints.resolve_file(&file, |file, line| {
                self.debug_line(&debugger.sources, file, line)
            });
            for breakpoint in resolved.into_iter().filter(|_| debugger.features.notify_ok) {
                debugger.notifications.push(format!(
                    r#"<notify {} name="breakpoint_resolved">{breakpoint}</notify>"#,
                    super::protocol::NAMESPACES,
                ));
            }
        }
        let candidates = debugger.breakpoints.candidates(|kind| match kind {
            Kind::Line {
                file: name,
                resolved,
                requested,
                ..
            } => line && name == &file && resolved.unwrap_or(*requested) == position.line,
            Kind::Call(name) => entered && name.eq_ignore_ascii_case(&frame.function().name),
            Kind::Return(name) => returning && name.eq_ignore_ascii_case(&frame.function().name),
            Kind::Exception(_) => false,
        });
        let force = std::mem::take(&mut debugger.force_break);
        let stepping = line && debugger.stepping(position.depth);
        if force || stepping || !candidates.is_empty() {
            debugger.stop = Some(super::session::Stop {
                location: format!(
                    r#"<xdebug:message filename="{}" lineno="{}"/>"#,
                    escape(&file),
                    position.line,
                ),
                line: position.line,
                unconditional: force || stepping,
                candidates,
            });
        }
        debugger.stop.is_some() || !debugger.notifications.is_empty()
    }

    pub(super) fn debug_line(&self, sources: &Sources, file: &str, requested: u32) -> Option<u32> {
        self.code
            .loaded_units()
            .filter(|unit| sources.uri(unit) == file)
            .flat_map(|unit| std::iter::once(&unit.main).chain(&unit.functions))
            .flat_map(|function| function.instructions.iter().zip(&function.lines))
            .filter(|(instruction, _)| is_breakable(**instruction))
            .map(|(_, line)| *line)
            .filter(|line| *line >= requested && *line != 0)
            .min()
    }
}

fn is_breakable(instruction: Instruction) -> bool {
    !matches!(
        instruction,
        Instruction::AttributeDiagnostics
            | Instruction::Nop
            | Instruction::CheckType { .. }
            | Instruction::ReceiveDefault { .. }
            | Instruction::GeneratorStart
    )
}
