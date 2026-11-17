//! Use a local directly when an adjacent consuming instruction cannot change it.
//!
//! Input and output are lowered bytecode; removed snapshots become Nops for the
//! subsequent target remapping pass. No read crosses another PHP expression.
//!
//! Input bytecode retains stable offsets during this pass. Replacements leave
//! Nops for later compaction. For example, Move(r1, r0); Return(r1) becomes
//! Nop; Return(r0) when r0 is a local and r1 a temporary.

use std::collections::HashSet;

use phera_bytecode::{Function, Instruction};

use crate::passes::{integer_operands::dead_after, optimize::target};

/// Remove adjacent local snapshots whose temporary has no later reader.
pub(crate) fn optimize(function: &mut Function) {
    let boundaries: HashSet<_> = function
        .instructions
        .iter()
        .filter_map(target)
        .map(|target| target.index())
        .collect();
    for index in 1..function.instructions.len() {
        if boundaries.contains(&index) {
            continue;
        }
        let Instruction::Move { dst, src } = function.instructions[index - 1] else {
            continue;
        };
        if dst.index() < function.locals.len() || src.index() >= function.locals.len() {
            continue;
        }
        if matches!(function.instructions[index], Instruction::Return { src: value } if value == dst)
        {
            function.instructions[index - 1] = Instruction::Nop;
            function.instructions[index] = Instruction::Return { src };
            continue;
        }
        if !dead_after(function, index, dst) {
            continue;
        }
        let replacement = match function.instructions[index] {
            Instruction::AssignOp {
                op,
                local,
                src: operand,
            } if operand == dst => Instruction::AssignOp { op, local, src },
            Instruction::AssignPath { path, src: operand } if operand == dst => {
                Instruction::AssignPath { path, src }
            }
            _ => continue,
        };
        function.instructions[index - 1] = Instruction::Nop;
        function.instructions[index] = replacement;
    }
}

/// A single append step needs no runtime bytecode-path lookup.
pub(crate) fn specialize_appends(function: &mut Function) {
    for instruction in &mut function.instructions {
        let Instruction::AssignPath { path, src } = *instruction else {
            continue;
        };
        let Some(path) = function.paths.get(path.index()) else {
            continue;
        };
        let phera_bytecode::PathBase::Register(local) = path.base else {
            continue;
        };
        if path.steps.as_slice() == [phera_bytecode::Step::Append] {
            *instruction = Instruction::AppendLocal { local, src };
        }
    }
}

/// Preserve a compiled fallback while exposing one register-based array index to the scalar loop.
pub(crate) fn specialize_indexes(function: &mut Function) {
    for instruction in &mut function.instructions {
        let Instruction::Fetch { dst, path, mode } = *instruction else {
            continue;
        };
        if mode == phera_bytecode::FetchMode::Destructure {
            continue;
        }
        let Some(definition) = function.paths.get(path.index()) else {
            continue;
        };
        let phera_bytecode::PathBase::Register(base) = definition.base else {
            continue;
        };
        let [phera_bytecode::Step::Index(index)] = definition.steps.as_slice() else {
            continue;
        };
        *instruction = Instruction::FetchIndex {
            dst,
            base,
            index: *index,
            path,
            mode,
        };
    }
}
