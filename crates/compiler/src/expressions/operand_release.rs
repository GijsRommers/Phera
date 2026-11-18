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
//! instruction, inside a loop on every round.

use std::ops::Range;

use phera_bytecode::Reg;

use crate::Compiler;
use crate::ir::{ArgumentSource, Binary, Instruction, Target, Unary};

/// The temporaries of one expression: the registers allocated and the
/// instructions emitted while it was compiled.
pub(crate) struct Temporaries {
    registers: Range<usize>,
    instructions: Range<usize>,
}

impl Compiler<'_> {
    /// Where the next expression's temporaries begin.
    pub(crate) fn temporaries_start(&self) -> (usize, usize) {
        (self.next, self.position())
    }

    /// The temporaries of the expression compiled since `start`.
    pub(crate) fn temporaries_since(&self, (first, start): (usize, usize)) -> Temporaries {
        Temporaries {
            registers: first..self.next,
            instructions: start..self.position(),
        }
    }

    /// Clears the temporaries a call was built from, right after the call;
    /// `result` is the call's own result and stays.
    pub(crate) fn release_call_operands(
        &mut self,
        start: (usize, usize),
        result: Reg,
        line: usize,
    ) {
        let operands = self.temporaries_since(start);
        for register in self.owning(&operands, Some(result)) {
            self.emit(Instruction::Clear { register }, line);
            self.released.insert(register.0);
        }
    }

    /// Clears the temporaries of an expression that has been used (a
    /// condition, a loop subject). A branch calls this on each way out.
    pub(crate) fn release_temporaries(&mut self, temporaries: &Temporaries, line: usize) {
        for register in self.owning(temporaries, None) {
            self.emit(Instruction::Clear { register }, line);
        }
    }

    /// The temporaries that may own an object.
    fn owning(&self, temporaries: &Temporaries, keep: Option<Reg>) -> Vec<Reg> {
        let code = self
            .function
            .instructions
            .get(temporaries.instructions.clone())
            .unwrap_or_default();
        let merges = code.iter().filter_map(jump_target).any(|target| {
            temporaries.instructions.contains(&target) && target > temporaries.instructions.start
        });
        temporaries
            .registers
            .clone()
            .map(|index| Reg(index as u16))
            .filter(|register| Some(*register) != keep)
            .filter(|register| !self.locals.values().any(|local| local == register))
            .filter(|register| !holds_a_scalar(code, *register, merges))
            .collect()
    }
}

/// Whether `register` holds a scalar after `code`. In straight-line code
/// its last write decides; where paths merge, every write must be scalar.
/// An instruction that mentions the register in a way not known here
/// counts as writing anything into it.
fn holds_a_scalar(code: &[Instruction], register: Reg, merges: bool) -> bool {
    let mut scalar = false;
    for instruction in code {
        let written = match scalar_write(instruction, register) {
            Some(result) => Some(result),
            None if reads_only(instruction, register) => None,
            None => Some(false),
        };
        if let Some(result) = written {
            if merges && !result {
                return false;
            }
            scalar = result;
        }
    }
    scalar
}

/// Where a jump inside an expression can land.
fn jump_target(instruction: &Instruction) -> Option<usize> {
    match instruction {
        Instruction::Jump(Target(target))
        | Instruction::Branch {
            target: Target(target),
            ..
        }
        | Instruction::BranchNull {
            target: Target(target),
            ..
        } => Some(*target),
        _ => None,
    }
}

/// `Some(true)` when the instruction writes a scalar into `register`,
/// `Some(false)` when it writes something else there, `None` when it is not
/// one of the instructions that compute scalars.
fn scalar_write(instruction: &Instruction, register: Reg) -> Option<bool> {
    match instruction {
        // Every constant (`crate::ir::Constant`) is a scalar or `[]`.
        Instruction::Load { destination, .. } if *destination == register => Some(true),
        Instruction::Binary {
            destination,
            operator,
            ..
        } if *destination == register => Some(*operator != Binary::Add),
        Instruction::Unary {
            destination,
            operator,
            ..
        } if *destination == register => Some(!matches!(operator, Unary::Array | Unary::Object)),
        _ => None,
    }
}

/// Whether the instruction leaves `register` alone or only reads it.
fn reads_only(instruction: &Instruction, register: Reg) -> bool {
    let mut mentioned = false;
    let mut probe = instruction.clone();
    probe.map_registers(|candidate| {
        mentioned |= candidate == register;
        candidate
    });
    if !mentioned {
        return true;
    }
    match instruction {
        Instruction::Send { argument } => {
            matches!(argument.source, ArgumentSource::Value(value) if value == register)
        }
        Instruction::Binary {
            destination,
            left,
            right,
            ..
        } => *destination != register && (*left == register || *right == register),
        Instruction::Unary {
            destination,
            source,
            ..
        } => *destination != register && *source == register,
        Instruction::Branch { value, .. } | Instruction::BranchNull { value, .. } => {
            *value == register
        }
        Instruction::Echo(value) => *value == register,
        _ => false,
    }
}
