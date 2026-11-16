//! Remove scalar temporary clears and dead statement results within basic blocks.
//!
//! Heap values must still be cleared promptly for copy-on-write. Facts are reset
//! at every control-flow boundary, so no branch-dependent type is assumed.

use std::collections::HashSet;

use phera_bytecode::{BinaryOp, Function, Instruction, Target};

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
}

fn target(instruction: &Instruction) -> Option<Target> {
    let mut instruction = *instruction;
    target_mut(&mut instruction).copied()
}

fn target_mut(instruction: &mut Instruction) -> Option<&mut Target> {
    match instruction {
        Instruction::Jump { target }
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
