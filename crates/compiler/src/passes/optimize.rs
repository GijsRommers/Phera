//! Remove scalar temporary clears and dead statement results within basic blocks.
//!
//! Heap values must still be cleared promptly for copy-on-write. Facts are reset
//! at every control-flow boundary, so no branch-dependent type is assumed.
//!
//! Input is emitted bytecode with valid register and target indices. Output
//! retains evaluation order and remaps all targets after removed instructions.
//! For example, LoadInt followed by Binary can become BinaryInt when the
//! literal temporary is proven dead; scalar-only Clear instructions disappear.

use std::collections::HashSet;

use phera_bytecode::{BinaryOp, Function, Instruction, Target};

use crate::passes::integer_operands::fuse_integer_operands;

/// Optimize operands in place while retaining source locations and valid branch targets.
pub(crate) fn optimize(function: &mut Function) {
    let boundaries: HashSet<_> = function
        .instructions
        .iter()
        .filter_map(target)
        .map(|target| target.index())
        .collect();
    let mut scalar = vec![false; function.registers as usize];
    let mut offsets = Vec::new();
    let mut instructions = Vec::new();
    let mut lines = Vec::new();
    for (index, instruction) in function.instructions.iter().copied().enumerate() {
        offsets.push(instructions.len() as u32);
        if boundaries.contains(&index) {
            scalar.fill(false);
        }
        let remove = match instruction {
            Instruction::Nop => true,
            Instruction::Clear { reg } => scalar.get(reg.index()).copied().unwrap_or(false),
            Instruction::Move { dst, .. } => function
                .instructions
                .iter()
                .skip(index + 1)
                .take_while(|instruction| matches!(instruction, Instruction::Clear { .. }))
                .any(|instruction| matches!(instruction,Instruction::Clear { reg } if *reg == dst)),
            _ => false,
        };
        update_scalar_facts(&mut scalar, instruction);
        if !remove {
            instructions.push(instruction);
            lines.push(function.lines.get(index).copied().unwrap_or(1));
        }
    }
    offsets.push(instructions.len() as u32);
    for instruction in &mut instructions {
        if let Some(target) = target_mut(instruction) {
            target.0 = offsets.get(target.index()).copied().unwrap_or(target.0);
        }
    }
    function.instructions = instructions;
    function.lines = lines;
    fuse_integer_operands(function);
    crate::passes::direct_operands::optimize(function);
    crate::passes::integer_operands::fuse_integer_branches(function);
    crate::passes::direct_operands::specialize_appends(function);
    compact_nops(function);
}

/// Read the target of a control-flow instruction when it has one.
pub(crate) fn target(instruction: &Instruction) -> Option<Target> {
    let mut instruction = *instruction;
    target_mut(&mut instruction).copied()
}

/// Expose the target operand for offset remapping after instruction removal.
pub(crate) fn target_mut(instruction: &mut Instruction) -> Option<&mut Target> {
    match instruction {
        Instruction::BinaryIntBranch { target, .. }
        | Instruction::Jump { target }
        | Instruction::JumpIf { target, .. }
        | Instruction::JumpIfNot { target, .. }
        | Instruction::JumpIfNull { target, .. }
        | Instruction::JumpIfNotNull { target, .. } => Some(target),
        Instruction::ReceiveDefault { passed, .. } => Some(passed),
        Instruction::BindStatic { initialized, .. } => Some(initialized),
        Instruction::IterNext { end, .. } | Instruction::IterNextKeyed { end, .. } => Some(end),
        _ => None,
    }
}

/// Preserve source locations and targets after integer fusion removes loads.
fn compact_nops(function: &mut Function) {
    let mut offsets = Vec::with_capacity(function.instructions.len() + 1);
    let mut count = 0;
    for instruction in &function.instructions {
        offsets.push(count);
        if !matches!(instruction, Instruction::Nop) {
            count += 1
        }
    }
    offsets.push(count);
    let mut lines = Vec::new();
    let mut instructions = Vec::new();
    for (index, mut instruction) in function.instructions.iter().copied().enumerate() {
        if matches!(instruction, Instruction::Nop) {
            continue;
        }
        if let Some(target) = target_mut(&mut instruction) {
            target.0 = offsets.get(target.index()).copied().unwrap_or(target.0);
        }
        instructions.push(instruction);
        lines.push(function.lines.get(index).copied().unwrap_or(1));
    }
    function.instructions = instructions;
    function.lines = lines;
}

/// Propagate only facts already recognized by the original scalar-clear pass.
fn update_scalar_facts(scalar: &mut [bool], instruction: Instruction) {
    match instruction {
        Instruction::LoadInt { dst, .. }
        | Instruction::LoadBool { dst, .. }
        | Instruction::LoadNull { dst } => set_scalar(scalar, dst, true),
        Instruction::Move { dst, src } | Instruction::Assign { local: dst, src } => {
            set_scalar(scalar, dst, is_scalar(scalar, src))
        }
        Instruction::Binary { op, dst, lhs, rhs } => set_scalar(
            scalar,
            dst,
            op != BinaryOp::Concat
                && (op != BinaryOp::Add || is_scalar(scalar, lhs) || is_scalar(scalar, rhs)),
        ),
        Instruction::AssignOp { op, local, src } => set_scalar(
            scalar,
            local,
            op != BinaryOp::Concat
                && (op != BinaryOp::Add || is_scalar(scalar, local) || is_scalar(scalar, src)),
        ),
        Instruction::Clear { reg } => set_scalar(scalar, reg, false),
        _ => scalar.fill(false),
    }
}

/// Missing facts are unknown, so they can never justify removing a clear.
fn is_scalar(scalar: &[bool], register: phera_bytecode::Reg) -> bool {
    scalar.get(register.index()).copied().unwrap_or(false)
}

/// Record a scalar fact without unchecked indexing.
fn set_scalar(scalar: &mut [bool], register: phera_bytecode::Reg, value: bool) {
    if let Some(slot) = scalar.get_mut(register.index()) {
        *slot = value;
    }
}
