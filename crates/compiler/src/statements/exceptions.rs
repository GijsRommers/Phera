//! Compile table-driven catches and single-copy finally subroutines.
//!
//! `try { return work(); } finally { cleanup(); }` becomes a saved result,
//! CheckReturn, FinallyCall(Return), ReturnChecked(result), then cleanup and
//! FinallyReturn. Normal
//! completion uses FinallyCall(Fallthrough). On exception the handler table
//! enters the same body with an exceptional continuation. A return in cleanup
//! emits FinallyDiscard before its own return, replacing the original outcome.

use phera_ast as ast;
use phera_bytecode::{FinallyEntry, Handler, Reg};

use crate::ir::{Instruction, Place, PlaceScope, Target};
use crate::policies::{DeclarationTiming, NameKind};
use crate::{CompileError, Compiler};

/// Whether an exit must call finally or discard a continuation already executing it.
#[derive(Clone, Copy)]
pub(crate) enum FinallyPhase {
    Pending,
    Running,
}

/// Lexical cleanup state is compile-only; entering a PHP try emits no instruction.
pub(crate) struct FinallyScope {
    pub(crate) state: Reg,
    pub(crate) loop_depth: usize,
    pub(crate) phase: FinallyPhase,
    calls: Vec<usize>,
}

impl Compiler<'_> {
    /// Handlers append after nested handlers, giving innermost-first search order.
    pub(crate) fn compile_try(
        &mut self,
        body: &[ast::Statement],
        catches: &[ast::Catch],
        finally: &Option<ast::Block>,
        line: usize,
    ) -> Result<(), CompileError> {
        if catches.is_empty() && finally.is_none() {
            return Err(self.error("Cannot use try without catch or finally", line));
        }
        let state = finally
            .as_ref()
            .map(|_| self.allocate_register())
            .transpose()?;
        if let Some(state) = state {
            self.finalies.push(FinallyScope {
                state,
                loop_depth: self.loops.len(),
                phase: FinallyPhase::Pending,
                calls: Vec::new(),
            });
        }
        let exception = self.allocate_register()?;
        let watermark = self.next;
        let start = self.position();
        self.block(body, DeclarationTiming::AtExecution)?;
        let end = self.position();
        let mut exits = vec![self.emit(Instruction::Jump(Target(0)), line)];
        let catch_start = self.position();
        self.compile_catches(catches, exception, &mut exits, line)?;
        if !catches.is_empty() {
            self.emit(Instruction::Throw { source: exception }, line);
            self.function.handlers.push(self.exception_handler(
                start,
                end,
                Some(catch_start),
                None,
                exception,
                watermark,
            ));
        }
        let normal = self.position();
        for exit in exits {
            self.patch(exit, normal);
        }
        if let Some(state) = state {
            self.emit_finally_exit(self.finalies.len() - 1, FinallyEntry::Fallthrough, line);
            let after = self.emit(Instruction::Jump(Target(0)), line);
            let finally_start = self.position();
            self.function.handlers.push(self.exception_handler(
                start,
                normal,
                None,
                Some(finally_start),
                state,
                watermark,
            ));
            self.compile_finally(finally.as_deref().unwrap_or_default(), state, line)?;
            self.patch(after, self.position());
        }
        Ok(())
    }

    /// Unknown catch names are ordinary negative instanceof tests, resolved only on entry.
    fn compile_catches(
        &mut self,
        catches: &[ast::Catch],
        exception: Reg,
        exits: &mut Vec<usize>,
        line: usize,
    ) -> Result<(), CompileError> {
        for catch in catches {
            if catch.variable.as_deref() == Some("this") {
                return Err(self.error("Cannot re-assign $this", line));
            }
            let mut matches = Vec::new();
            for name in &catch.types {
                if matches!(
                    name.text.to_ascii_lowercase().as_str(),
                    "self" | "parent" | "static"
                ) {
                    return Err(self.error("Bad class name in the catch statement", line));
                }
                let destination = self.allocate_register()?;
                let (class, _) = self.resolve(name, NameKind::Class);
                self.emit(
                    Instruction::InstanceOf {
                        destination,
                        source: exception,
                        class,
                    },
                    line,
                );
                matches.push(self.emit(
                    Instruction::Branch {
                        value: destination,
                        target: Target(0),
                        when: true,
                    },
                    line,
                ));
            }
            let next = self.emit(Instruction::Jump(Target(0)), line);
            for branch in matches {
                self.patch(branch, self.position());
            }
            if let Some(variable) = &catch.variable {
                let base = self.local(variable)?;
                self.emit(
                    Instruction::Write {
                        place: Place {
                            base,
                            scope: PlaceScope::Local,
                            path: Vec::new(),
                        },
                        source: exception,
                    },
                    line,
                );
            }
            self.emit(
                Instruction::Clear {
                    register: exception,
                },
                line,
            );
            self.block(&catch.body, DeclarationTiming::AtExecution)?;
            exits.push(self.emit(Instruction::Jump(Target(0)), line));
            self.patch(next, self.position());
        }
        Ok(())
    }

    /// All call sites share one body and one continuation register per lexical finally.
    fn compile_finally(
        &mut self,
        body: &[ast::Statement],
        state: Reg,
        line: usize,
    ) -> Result<(), CompileError> {
        let target = self.position();
        if let Some(scope) = self.finalies.last_mut() {
            scope.phase = FinallyPhase::Running;
        }
        self.block(body, DeclarationTiming::AtExecution)?;
        self.emit(Instruction::FinallyReturn { state }, line);
        if let Some(scope) = self.finalies.pop() {
            for index in scope.calls {
                if let Some(Instruction::FinallyCall {
                    target: destination,
                    ..
                }) = self.function.instructions.get_mut(index)
                {
                    *destination = Target(target);
                }
            }
        }
        Ok(())
    }

    /// Structured exits run only finally scopes inside the loop being exited.
    pub(crate) fn emit_finally_exits(
        &mut self,
        loop_depth: Option<usize>,
        reason: FinallyEntry,
        line: usize,
    ) {
        for index in (0..self.finalies.len()).rev() {
            if loop_depth.is_none_or(|depth| self.finalies[index].loop_depth > depth) {
                self.emit_finally_exit(index, reason, line);
            }
        }
    }

    /// An exit from the currently executing finally supersedes its saved exception or return.
    fn emit_finally_exit(&mut self, index: usize, reason: FinallyEntry, line: usize) {
        let state = self.finalies[index].state;
        match self.finalies[index].phase {
            FinallyPhase::Pending => {
                let call = self.emit(
                    Instruction::FinallyCall {
                        state,
                        target: Target(0),
                        reason,
                    },
                    line,
                );
                self.finalies[index].calls.push(call);
            }
            FinallyPhase::Running => {
                self.emit(Instruction::FinallyDiscard { state }, line);
            }
        }
    }

    /// Locals and outer expression values survive; temporaries created in the try do not.
    fn exception_handler(
        &self,
        start: usize,
        end: usize,
        catch: Option<usize>,
        finally: Option<usize>,
        exception: Reg,
        watermark: usize,
    ) -> Handler {
        Handler {
            start: phera_bytecode::Target(start as u32),
            end: phera_bytecode::Target(end as u32),
            catch: catch.map(|target| phera_bytecode::Target(target as u32)),
            finally: finally.map(|target| phera_bytecode::Target(target as u32)),
            exception,
            cleanup: (watermark..self.peak)
                .map(|index| Reg(index as u16))
                .filter(|register| !self.locals.values().any(|local| local == register))
                .collect(),
        }
    }
}
