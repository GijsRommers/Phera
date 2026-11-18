//! Each PHP function (including the file body) has its own label namespace.
//! `goto done; echo 'skipped'; done:` patches the jump when that body ends.
//! A resolved exit retains the scopes shared with its target and uses the same
//! cleanup as break for the scopes it leaves.

use std::{collections::HashMap, ops::Range};

use phera_ast::Identifier;
use phera_bytecode::FinallyEntry;

use super::{
    exceptions::FinallyPhase,
    exits::{ExitDepth, ExitScope},
};
use crate::{
    CompileError, Compiler,
    ir::{Instruction, Target},
};

#[derive(Default)]
pub(crate) struct Labels {
    definitions: HashMap<String, Label>,
    pending: Vec<ForwardJump>,
}

struct Label {
    position: usize,
    scopes: Scopes,
}

struct ForwardJump {
    name: String,
    line: usize,
    position: usize,
    scopes: Scopes,
    cleanup: Vec<(ExitScope, Range<usize>)>,
}

struct Scopes {
    /// An entry position identifies a loop among its siblings.
    loops: Vec<usize>,
    finalies: Vec<FinallyLocation>,
}

struct FinallyLocation {
    start: usize,
    phase: FinallyPhase,
}

impl Scopes {
    fn retained_by(&self, target: &Self) -> Result<ExitDepth, &'static str> {
        if !self.loops.starts_with(&target.loops) {
            return Err("'goto' into loop or switch statement is disallowed");
        }

        let shared = self
            .finalies
            .iter()
            .zip(&target.finalies)
            .take_while(|(source, target)| source.start == target.start)
            .count();
        for (source, target) in self.finalies.iter().zip(&target.finalies).take(shared) {
            if source.phase != target.phase {
                return Err(if target.phase == FinallyPhase::Running {
                    "jump into a finally block is disallowed"
                } else {
                    "jump out of a finally block is disallowed"
                });
            }
        }
        let leaving = self.finalies[shared..]
            .iter()
            .find(|scope| scope.phase == FinallyPhase::Running);
        let entering = target.finalies[shared..]
            .iter()
            .find(|scope| scope.phase == FinallyPhase::Running);
        if let Some(scope) = leaving
            && entering.is_none_or(|target| scope.start <= target.start)
        {
            return Err("jump out of a finally block is disallowed");
        }
        if entering.is_some() {
            return Err("jump into a finally block is disallowed");
        }
        Ok(ExitDepth {
            loops: target.loops.len(),
            finalies: shared,
        })
    }
}

impl Compiler<'_> {
    fn label_scopes(&self) -> Scopes {
        Scopes {
            loops: self.loops.iter().map(|scope| scope.start).collect(),
            finalies: self
                .finalies
                .iter()
                .map(|scope| FinallyLocation {
                    start: scope.start,
                    phase: scope.phase,
                })
                .collect(),
        }
    }

    pub(super) fn compile_label(&mut self, name: &Identifier) -> Result<(), CompileError> {
        let label = Label {
            position: self.position(),
            scopes: self.label_scopes(),
        };
        let labels = self.labels.get_or_insert_with(Default::default);
        if labels
            .definitions
            .insert(name.name.clone(), label)
            .is_some()
        {
            return Err(self.error(
                format!("Label '{}' already defined", name.name),
                self.lines.line(name.span.start),
            ));
        }
        Ok(())
    }

    pub(super) fn compile_goto(&mut self, name: &Identifier) {
        let line = self.lines.line(name.span.start);
        let scopes = self.label_scopes();
        if let Some(target) = self
            .labels
            .as_ref()
            .and_then(|labels| labels.definitions.get(&name.name))
            && let Ok(retained) = scopes.retained_by(&target.scopes)
        {
            let position = target.position;
            self.emit_control_exits(retained, FinallyEntry::Goto, line, |_, _| {});
            self.emit(Instruction::Jump(Target(position)), line);
            return;
        }

        // Record potential cleanup at the source, so exceptions during cleanup
        // still see the source's enclosing handlers. An invalid backward jump
        // is diagnosed in source order with forward jumps after duplicate checks.
        let mut cleanup = Vec::new();
        self.emit_control_exits(
            ExitDepth::default(),
            FinallyEntry::Goto,
            line,
            |scope, sites| cleanup.push((scope, sites)),
        );
        let position = self.emit(Instruction::Jump(Target(0)), line);
        self.labels
            .get_or_insert_with(Default::default)
            .pending
            .push(ForwardJump {
                name: name.name.clone(),
                line,
                position,
                scopes,
                cleanup,
            });
    }

    pub(crate) fn finish_gotos(&mut self) -> Result<(), CompileError> {
        let Some(labels) = self.labels.take() else {
            return Ok(());
        };
        for jump in labels.pending {
            let label = labels.definitions.get(&jump.name).ok_or_else(|| {
                self.error(
                    format!("'goto' to undefined label '{}'", jump.name),
                    jump.line,
                )
            })?;
            let retained = jump
                .scopes
                .retained_by(&label.scopes)
                .map_err(|error| self.error(error, jump.line))?;
            for (scope, sites) in jump.cleanup {
                if !scope.is_exited(retained)
                    && let Some(instructions) = self.function.instructions.get_mut(sites)
                {
                    instructions.fill(Instruction::Nop);
                }
            }
            self.patch(jump.position, label.position);
        }
        Ok(())
    }
}
