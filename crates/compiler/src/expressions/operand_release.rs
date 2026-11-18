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
//! A register that provably holds a scalar (a literal, arithmetic, a
//! comparison) owns nothing that could have a destructor, so it is left to
//! the statement-end clear (or to the frame's end): clearing it would only
//! cost an instruction.

use phera_bytecode::Reg;

use crate::Compiler;
use crate::ir::{ArgumentSource, Binary, Instruction, Unary};

impl Compiler<'_> {
    /// Clears the temporaries allocated from register `first` on, while the
    /// call whose result is `result` was compiled, starting at instruction
    /// `start`.
    pub(crate) fn release_call_operands(
        &mut self,
        first: usize,
        start: usize,
        result: Reg,
        line: usize,
    ) {
        for register in self.owning_temporaries(first, start, Some(result)) {
            self.emit(Instruction::Clear { register }, line);
            self.released.insert(register.0);
        }
    }

    /// Clears the temporaries allocated from register `first` on, from
    /// instruction `start` on, once the expression that needed them (a
    /// condition, a loop subject) has been used. A branch calls this on
    /// each of its ways out.
    pub(crate) fn release_temporaries(&mut self, first: usize, start: usize, line: usize) {
        for register in self.owning_temporaries(first, start, None) {
            self.emit(Instruction::Clear { register }, line);
        }
    }

    /// The temporaries from `first` on that may own an object.
    fn owning_temporaries(&self, first: usize, start: usize, keep: Option<Reg>) -> Vec<Reg> {
        (first..self.next)
            .map(|index| Reg(index as u16))
            .filter(|register| Some(*register) != keep)
            .filter(|register| !self.locals.values().any(|local| local == register))
            .filter(|register| !self.holds_only_scalars(*register, start))
            .collect()
    }

    /// Whether every write to `register` since instruction `start` produces
    /// a scalar, and every other use only reads it.
    fn holds_only_scalars(&self, register: Reg, start: usize) -> bool {
        let mut written = false;
        for instruction in self.function.instructions.iter().skip(start) {
            match scalar_write(instruction, register) {
                Some(true) => written = true,
                Some(false) => return false,
                None if reads_only(instruction, register) => {}
                None => return false,
            }
        }
        written
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
