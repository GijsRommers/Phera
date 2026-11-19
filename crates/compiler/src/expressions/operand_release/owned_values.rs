//! Which temporaries own nothing that a clear would free.
//!
//! ```php
//! if ($i < $n && $items[$i]->ready()) { ... }
//! // `$i < $n` gives a boolean, and the copies of `$i` and `$n` it reads are
//! // copies of variables: clearing those registers frees nothing.
//! ```
//!
//! A forward pass over an expression's compiled code follows each register
//! to the end of the expression; see [`Expression::holds_a_scalar`].

use std::collections::HashMap;

use phera_bytecode::Reg;

use crate::ir::{ArgumentSource, Binary, Instruction, PlaceScope, Target, Unary};

/// The compiled code of one expression, and the function's variables.
pub(super) struct Expression<'a> {
    pub(super) code: &'a [Instruction],
    pub(super) locals: &'a [Reg],
    /// The registers own nothing before the code runs.
    pub(super) clean_start: bool,
}

impl Expression<'_> {
    /// Whether `register` holds a scalar (or a copy of an untouched variable)
    /// after the expression's code, which starts at position `start`, whichever
    /// way control goes through it. A forward
    /// pass: the state after each write, carried along jumps (an expression
    /// only jumps forward, past its short-circuited parts) and merged where
    /// they land. An instruction that mentions the register in a way not known
    /// here counts as writing anything into it, and so does a path that never
    /// writes it, unless the expression starts clean.
    pub(super) fn holds_a_scalar(&self, start: usize, register: Reg) -> bool {
        let code = self.code;
        let end = start + code.len();
        let mut landing: HashMap<usize, bool> = HashMap::new();
        // `None` after an unconditional jump, until something lands.
        let mut state = Some(self.clean_start);
        let mut leaving = true;
        for (offset, instruction) in code.iter().enumerate() {
            let here = start + offset;
            if let Some(&landed) = landing.get(&here) {
                state = Some(state.map_or(landed, |current| current && landed));
            }
            let Some(mut scalar) = state else {
                continue;
            };
            match self.scalar_write(instruction, register) {
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

    /// `Some(true)` when the instruction writes a scalar into `register`, or a
    /// copy of a variable this expression only reads; `Some(false)` when it
    /// writes something else there; `None` when it is neither kind.
    fn scalar_write(&self, instruction: &Instruction, register: Reg) -> Option<bool> {
        match instruction {
            Instruction::Move {
                destination,
                source,
            } if *destination == register => Some(self.untouched_local(*source)),
            Instruction::Read {
                destination, place, ..
            } if *destination == register
                && place.scope == PlaceScope::Local
                && place.path.is_empty() =>
            {
                Some(self.untouched_local(place.base))
            }
            _ => scalar_write(instruction, register),
        }
    }

    /// Whether `register` is a variable that the expression never writes.
    fn untouched_local(&self, register: Reg) -> bool {
        self.locals.contains(&register)
            && self
                .code
                .iter()
                .all(|instruction| reads_only(instruction, register))
    }
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
        Instruction::Move {
            destination,
            source,
        } => *destination != register && *source == register,
        // Reading a variable's place mentions its base and index registers
        // only to read them (a `$this` place writes its scratch base).
        Instruction::Read {
            destination, place, ..
        } => *destination != register && place.scope == PlaceScope::Local,
        _ => false,
    }
}
