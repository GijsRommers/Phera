//! Remove scalar temporary clears and dead statement results within basic blocks.
//!
//! Heap values must still be cleared promptly for copy-on-write. Facts are reset
//! at every control-flow boundary, so no branch-dependent type is assumed.

use std::collections::HashSet;

use phera_bytecode::{BinaryOp, Function, Instruction, Target};

use crate::integer_operands::fuse_integer_operands;

pub(super) fn optimize(function: &mut Function) {
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
        match instruction {
            Instruction::LoadInt { dst, .. }
            | Instruction::LoadBool { dst, .. }
            | Instruction::LoadNull { dst } => scalar[dst.index()] = true,
            Instruction::Move { dst, src } | Instruction::Assign { local: dst, src } => {
                scalar[dst.index()] = scalar[src.index()]
            }
            Instruction::Binary { op, dst, lhs, rhs } => {
                scalar[dst.index()] = op != BinaryOp::Concat
                    && (op != BinaryOp::Add || scalar[lhs.index()] || scalar[rhs.index()])
            }
            Instruction::AssignOp { op, local, src } => {
                scalar[local.index()] = op != BinaryOp::Concat
                    && (op != BinaryOp::Add || scalar[local.index()] || scalar[src.index()])
            }
            Instruction::Clear { reg } => scalar[reg.index()] = false,
            _ => scalar.fill(false),
        }
        if !remove {
            instructions.push(instruction);
            lines.push(function.lines[index]);
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
    crate::direct_operands::optimize(function);
    crate::integer_operands::fuse_integer_branches(function);
    crate::direct_operands::specialize_appends(function);
    compact_nops(function);
}

pub(super) fn target(instruction: &Instruction) -> Option<Target> {
    let mut instruction = *instruction;
    target_mut(&mut instruction).copied()
}

pub(super) fn target_mut(instruction: &mut Instruction) -> Option<&mut Target> {
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
        lines.push(function.lines[index]);
    }
    function.instructions = instructions;
    function.lines = lines;
}
