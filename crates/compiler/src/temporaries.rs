//! Handing temporary registers back once their values have been used.
//!
//! Registers are numbered with 16 bits, so one function has at most 65,534 of
//! them. Generated PHP easily needs more temporaries than that if each one is
//! kept to the end of the function:
//!
//! ```php
//! <?php
//! $table = [1 => 'a', 2 => 'b', /* ... 100,000 more ... */];
//! $html = "<td>$a</td><td>$b</td>/* ... */";
//! $a++; $a++; /* ... a million statements ... */
//! ```
//!
//! Each element, argument, string part or statement only needs its
//! temporaries until its value has been pushed, sent, concatenated or
//! discarded. A [`TemporaryMark`] taken before such a step lets the registers
//! the step allocated be allocated again by the next step.
//!
//! A named local can never be handed back: once `$x` has a register, every
//! later mention of `$x` uses it. Locals are always allocated above every
//! register used so far, so a step that created a local keeps everything up to
//! and including that local, and hands back only what lies above it.

use std::collections::HashSet;

use phera_bytecode::Reg;

use crate::ir::Instruction;
use crate::{CompileError, Compiler};

/// The allocation state before one step of a list or a chain.
#[derive(Clone, Copy)]
pub(crate) struct TemporaryMark {
    next: usize,
    locals: usize,
}

impl Compiler<'_> {
    /// Remembers which registers are in use before a step.
    pub(crate) fn mark_temporaries(&self) -> TemporaryMark {
        TemporaryMark {
            next: self.next,
            locals: self.function.locals.len(),
        }
    }

    /// Hands back the temporaries allocated since `mark`. Only for steps
    /// whose temporaries are all dead: nothing allocated after `mark` may be
    /// read again, except a local.
    pub(crate) fn release_temporaries(&mut self, mark: TemporaryMark) {
        let newest_local = self.function.locals[mark.locals.min(self.function.locals.len())..]
            .iter()
            .filter_map(|name| self.locals.get(name))
            .map(|register| register.index() + 1)
            .max();
        self.next = newest_local.map_or(mark.next, |end| end.max(mark.next));
    }

    /// Runs `body`, an expression statement, then clears every temporary it
    /// left a value in and hands them all back.
    ///
    /// PHP frees a statement's temporaries when the statement ends, which is
    /// when `$a = new Foo; f(new Bar);` destroys the `Bar`. A step inside the
    /// statement may already have handed its registers back, still holding
    /// values, so the clearing covers everything up to the highest register
    /// the statement reached, not only up to where allocation ended.
    pub(crate) fn clear_temporaries_after(
        &mut self,
        line: usize,
        body: impl FnOnce(&mut Self) -> Result<(), CompileError>,
    ) -> Result<(), CompileError> {
        let mark = self.mark_temporaries();
        let outer_reach = std::mem::replace(&mut self.reach, self.next);
        let result = body(self);
        let end = self.reach.max(self.next);
        self.reach = outer_reach.max(end);
        result?;

        let new_locals: HashSet<usize> = self.function.locals
            [mark.locals.min(self.function.locals.len())..]
            .iter()
            .filter_map(|name| self.locals.get(name))
            .map(|register| register.index())
            .collect();
        for index in (mark.next..end).filter(|index| !new_locals.contains(index)) {
            if let Ok(index) = u16::try_from(index) {
                self.emit(
                    Instruction::Clear {
                        register: Reg(index),
                    },
                    line,
                );
            }
        }
        self.release_temporaries(mark);
        Ok(())
    }
}
