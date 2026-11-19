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

use std::collections::HashMap;
use std::ops::Range;

use phera_bytecode::Reg;

use crate::Compiler;
use crate::ir::{ArgumentSource, Binary, Instruction, PlaceScope, Target, Unary};

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
        for register in self.owning(&operands, Some(result)) {
            self.emit(Instruction::Clear { register }, line);
            self.released.insert(register.0);
        }
    }

    /// Clears the temporaries of an expression that has been used (a
    /// condition, a loop subject). A branch calls this on each way out.
    pub(crate) fn clear_owning_temporaries(&mut self, temporaries: &Temporaries, line: usize) {
        for register in self.owning(temporaries, None) {
            self.emit(Instruction::Clear { register }, line);
        }
    }

    /// The temporaries that may own an object.
    fn owning(&self, temporaries: &Temporaries, keep: Option<Reg>) -> Vec<Reg> {
        let code = self
            .function
            .instructions
            .get(temporaries.instructions())
            .unwrap_or_default();
        temporaries
            .registers()
            .map(|index| Reg(index as u16))
            .filter(|register| Some(*register) != keep)
            .filter(|register| !self.locals.values().any(|local| local == register))
            .filter(|register| !holds_a_scalar(code, temporaries.instructions.0, *register))
            .collect()
    }
}

/// Whether `register` holds a scalar after `code`, the instructions from
/// position `start` on, whichever way control goes through them. A forward
/// pass: the state after each write, carried along jumps (an expression
/// only jumps forward, past its short-circuited parts) and merged where
/// they land. An instruction that mentions the register in a way not known
/// here counts as writing anything into it, and so does a path that never
/// writes it.
fn holds_a_scalar(code: &[Instruction], start: usize, register: Reg) -> bool {
    let end = start + code.len();
    let mut landing: HashMap<usize, bool> = HashMap::new();
    // `None` after an unconditional jump, until something lands.
    let mut state = Some(false);
    let mut leaving = true;
    for (offset, instruction) in code.iter().enumerate() {
        let here = start + offset;
        if let Some(&landed) = landing.get(&here) {
            state = Some(state.map_or(landed, |current| current && landed));
        }
        let Some(mut scalar) = state else {
            continue;
        };
        match scalar_write(instruction, register) {
            Some(result) => scalar = result,
            None if reads_only(instruction, register) => {}
            None => scalar = false,
        }
        if let Some(target) = jump_target(instruction) {
            if here < target && target < end {
                let merged = landing
                    .get(&target)
                    .map_or(scalar, |other| *other && scalar);
                landing.insert(target, merged);
            } else {
                leaving &= scalar;
            }
            if matches!(instruction, Instruction::Jump(_)) {
                state = None;
                continue;
            }
        }
        state = Some(scalar);
    }
    leaving && state.unwrap_or(true)
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
        // `$i++` gives a number or a string (objects and arrays throw);
        // `instanceof` gives a boolean.
        Instruction::IncPlace { destination, .. }
        | Instruction::InstanceOf { destination, .. }
        | Instruction::DynamicInstanceOf { destination, .. }
            if *destination == register =>
        {
            Some(true)
        }
        // `$this`, and the scratch register a `$this->property` path keeps
        // it in: the frame holds `$this`, so releasing a copy frees nothing.
        Instruction::This { destination } if *destination == register => Some(true),
        _ if this_scratch(instruction) == Some(register) => Some(true),
        _ => None,
    }
}

/// The register that holds `$this` for the instruction's `$this->...` path.
fn this_scratch(instruction: &Instruction) -> Option<Reg> {
    let place = match instruction {
        Instruction::Read { place, .. }
        | Instruction::Write { place, .. }
        | Instruction::WriteFromLocal { place, .. }
        | Instruction::IncPlace { place, .. }
        | Instruction::Compound { place, .. }
        | Instruction::Reference { place, .. }
        | Instruction::Rebind { place, .. }
        | Instruction::Unset(place) => place,
        Instruction::Send { argument } => match &argument.source {
            ArgumentSource::Place(place) => place,
            ArgumentSource::Value(_) => return None,
        },
        _ => return None,
    };
    (place.scope == PlaceScope::This).then_some(place.base)
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
