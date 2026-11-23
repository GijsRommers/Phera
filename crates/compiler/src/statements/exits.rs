//! A nonlocal exit unwinds lexical scopes from the inside out.
//! In `try { foreach ($items as $item) { goto done; } } finally { cleanup(); } done:`
//! the iterator is released before the finally outside that foreach is called.

use std::ops::Range;

use phera_bytecode::{FinallyEntry, Reg};

use crate::{Compiler, ir::Instruction};

/// Scopes kept by the destination of a jump.
#[derive(Clone, Copy, Default)]
pub(super) struct ExitDepth {
    pub(super) loops: usize,
    pub(super) finally_blocks: usize,
}

#[derive(Clone, Copy)]
pub(super) enum ExitScope {
    Loop(usize),
    Finally(usize),
}

impl ExitScope {
    pub(super) fn is_exited(self, retained: ExitDepth) -> bool {
        match self {
            Self::Loop(index) => index >= retained.loops,
            Self::Finally(index) => index >= retained.finally_blocks,
        }
    }
}

/// Ordinary loops have no temporary state to release.
#[derive(Clone, Copy)]
pub(crate) enum LoopCleanup {
    /// The iterator, and the temporaries of the subject and of the
    /// registers the loop copies values into.
    Foreach {
        iterator: Reg,
        owned: crate::expressions::operand_release::Temporaries,
        subject: Option<Reg>,
    },
    Switch(Reg),
}

impl Compiler<'_> {
    /// Emit shared cleanup and optionally record sites whose destination is still unknown.
    pub(super) fn emit_control_exits(
        &mut self,
        retained: ExitDepth,
        reason: FinallyEntry,
        line: usize,
        mut record: impl FnMut(ExitScope, Range<usize>),
    ) {
        let mut loops = self.loops.len();
        for index in (retained.finally_blocks..self.finalies.len()).rev() {
            let inner = self.finalies[index].loop_depth.max(retained.loops);
            while loops > inner {
                loops -= 1;
                self.emit_loop_exit(loops, line, &mut record);
            }
            let start = self.position();
            self.emit_finally_exit(index, reason, line);
            record(ExitScope::Finally(index), start..self.position());
        }
        while loops > retained.loops {
            loops -= 1;
            self.emit_loop_exit(loops, line, &mut record);
        }
    }

    fn emit_loop_exit(
        &mut self,
        index: usize,
        line: usize,
        record: &mut impl FnMut(ExitScope, Range<usize>),
    ) {
        let Some(cleanup) = self.loops[index].cleanup else {
            return;
        };
        let start = self.position();
        match cleanup {
            LoopCleanup::Foreach {
                iterator,
                owned,
                subject,
            } => {
                self.emit(Instruction::IterEnd(iterator), line);
                self.clear_foreach_temporaries(&owned, subject, line);
            }
            LoopCleanup::Switch(register) => {
                self.emit(Instruction::Clear { register }, line);
            }
        }
        record(ExitScope::Loop(index), start..self.position());
    }
}
