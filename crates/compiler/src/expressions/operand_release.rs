//! Releasing a call's operands when the call returns.
//!
//! ```php
//! class A { function __destruct() { echo "freed\n"; } }
//! var_dump(get_class(new A));   // "freed", then string(1) "A"
//! if ((new A)->ok()) { ... }    // "freed" before the body runs
//! ```
//!
//! PHP frees the temporary values a call was built from (the receiver, the
//! closure, the arguments) as soon as the callee returns, so an object that
//! only lived in one of them is destroyed, and its destructor runs, before
//! the result is used. Phera evaluates each operand into a temporary
//! register and passes a copy, so the compiler clears those registers right
//! after the call. The registers are temporaries the compiler allocated
//! while it compiled the call; nothing reads them afterwards.
//!
//! The same holds for the temporaries of a condition (`if`, `while`,
//! `for`, `switch`) and of a `foreach` subject, which no statement-end clear
//! covers: they are cleared on each way out of the branch or loop.
//!
//! A register that provably holds a scalar when it is cleared (its last
//! write is a literal, arithmetic, a comparison) owns nothing that could
//! have a destructor, so it is left alone: clearing it would only cost an
//! instruction, inside a loop on every round. The same goes for a copy of a
//! variable that the expression does not assign (`$i < $n` reads `$n` into
//! a register): the variable still holds the value, so releasing the copy
//! frees nothing. (A function the expression calls could still change the
//! variable through a PHP reference; the copy then lives until its register
//! is reused or the statement-end clear, a rare and short delay.)

mod owned_values;

use std::ops::Range;

use phera_bytecode::Reg;

use crate::Compiler;
use crate::ir::Instruction;
use owned_values::Expression;

/// The temporaries of one expression: the registers allocated and the
/// instructions emitted while it was compiled (as `start..end` pairs, so
/// that the value can be copied into a loop's cleanup).
#[derive(Clone, Copy)]
pub(crate) struct Temporaries {
    registers: (usize, usize),
    instructions: (usize, usize),
}

impl Temporaries {
    fn registers(&self) -> Range<usize> {
        self.registers.0..self.registers.1
    }

    fn instructions(&self) -> Range<usize> {
        self.instructions.0..self.instructions.1
    }
}

/// Where an expression's temporaries begin; see
/// [`Compiler::temporaries_start`].
#[derive(Clone, Copy)]
pub(crate) struct TemporariesStart {
    next: usize,
    position: usize,
    /// The highest register the enclosing code had reached.
    outer_reach: usize,
}

impl Compiler<'_> {
    /// Where the next expression's temporaries begin. Registers are handed
    /// back and allocated again within an expression (see
    /// `crate::temporaries`), so the expression's registers are those from
    /// here up to the highest one it reaches.
    pub(crate) fn temporaries_start(&mut self) -> TemporariesStart {
        let start = TemporariesStart {
            next: self.next,
            position: self.position(),
            outer_reach: self.reach,
        };
        self.reach = self.next;
        start
    }

    /// The temporaries of the expression compiled since `start`.
    pub(crate) fn temporaries_since(&mut self, start: TemporariesStart) -> Temporaries {
        let end = self.reach.max(self.next);
        self.reach = start.outer_reach.max(end);
        Temporaries {
            registers: (start.next, end),
            instructions: (start.position, self.position()),
        }
    }

    /// Clears the temporaries a call was built from, right after the call;
    /// `result` is the call's own result and stays.
    pub(crate) fn release_call_operands(
        &mut self,
        start: TemporariesStart,
        result: Reg,
        line: usize,
    ) {
        let operands = self.temporaries_since(start);
        // A sibling operand may have handed a register back without
        // clearing it, so one this call never writes may own something.
        for register in self.owning(&operands, Some(result), false) {
            self.emit(Instruction::Clear { register }, line);
            self.released.insert(register.0);
        }
    }

    /// Clears the temporaries of an expression that has been used (a loop
    /// subject, a `switch` comparison). A branch calls this on each way out.
    pub(crate) fn clear_owning_temporaries(&mut self, temporaries: &Temporaries, line: usize) {
        for register in self.owning(temporaries, None, false) {
            self.emit(Instruction::Clear { register }, line);
        }
    }

    /// Clears the temporaries of an `if`, `while`, `do` or `for` condition,
    /// on each way out of the branch.
    ///
    /// A condition starts a statement, or a new round of a loop, when every
    /// free register is empty or holds what owns nothing: the previous
    /// statement's end cleared them, and the previous round cleared what it
    /// owned. So a register a short-circuited path never writes owns
    /// nothing either.
    pub(crate) fn clear_condition_temporaries(&mut self, temporaries: &Temporaries, line: usize) {
        for register in self.owning(temporaries, None, true) {
            self.emit(Instruction::Clear { register }, line);
        }
    }

    /// `clean_start`: the registers own nothing when the expression begins.
    fn owning(&self, temporaries: &Temporaries, keep: Option<Reg>, clean_start: bool) -> Vec<Reg> {
        let code = self
            .function
            .instructions
            .get(temporaries.instructions())
            .unwrap_or_default();
        let locals: Vec<Reg> = self.locals.values().copied().collect();
        let expression = Expression {
            code,
            locals: &locals,
            clean_start,
        };
        temporaries
            .registers()
            .map(|index| Reg(index as u16))
            .filter(|register| Some(*register) != keep)
            .filter(|register| !locals.contains(register))
            .filter(|register| !expression.holds_a_scalar(temporaries.instructions.0, *register))
            .collect()
    }
}
