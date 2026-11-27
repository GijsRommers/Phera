//! Use a local directly when an adjacent consuming instruction cannot change it.
//!
//! Input and output are lowered bytecode; removed snapshots become Nops for the
//! subsequent target remapping pass. No read crosses another PHP expression.
//!
//! Input bytecode retains stable offsets during this pass. Replacements leave
//! Nops for later compaction. For example, Move(r1, r0); Return(r1) becomes
//! Nop; Return(r0) when r0 is a local and r1 a temporary.

use phera_bytecode::{Function, Instruction};

use crate::passes::{integer_operands::dead_after, optimize::block_boundaries};

/// Remove adjacent local snapshots whose temporary has no later reader, and
/// fold temporary-to-temporary moves immediately before `Return`.
pub(crate) fn optimize(function: &mut Function) {
    let boundaries = block_boundaries(function);
    let locals = function.locals.len();
    let mut path_uses = vec![0u32; function.paths.len()];
    for instruction in &function.instructions {
        if let Some(path) = crate::passes::operand_regs::path_operand(*instruction)
            && let Some(count) = path_uses.get_mut(path.index())
        {
            *count += 1;
        }
    }
    for index in 1..function.instructions.len() {
        if boundaries.contains(&index) {
            continue;
        }
        let Instruction::Move { dst, src } = function.instructions[index - 1] else {
            continue;
        };
        if dst.index() < locals {
            continue;
        }
        if matches!(function.instructions[index], Instruction::Return { src: value } if value == dst)
        {
            function.instructions[index - 1] = Instruction::Nop;
            function.instructions[index] = Instruction::Return { src };
            continue;
        }
        if src.index() >= locals || !dead_after(function, index, dst) {
            continue;
        }
        match function.instructions[index] {
            Instruction::AssignOp {
                op,
                local,
                src: operand,
            } if operand == dst => {
                function.instructions[index - 1] = Instruction::Nop;
                function.instructions[index] = Instruction::AssignOp { op, local, src };
            }
            Instruction::AssignPath { path, src: operand } if operand == dst => {
                function.instructions[index - 1] = Instruction::Nop;
                function.instructions[index] = Instruction::AssignPath { path, src };
            }
            Instruction::FetchIndex {
                dst: out,
                base,
                index: key_reg,
                path,
                mode,
            } if key_reg == dst && path_uses.get(path.index()).copied() == Some(1) => {
                if let Some(definition) = function.paths.get_mut(path.index())
                    && let [phera_bytecode::Step::Index(step_reg)] = definition.steps.as_mut_slice()
                    && *step_reg == dst
                {
                    *step_reg = src;
                    function.instructions[index - 1] = Instruction::Nop;
                    function.instructions[index] = Instruction::FetchIndex {
                        dst: out,
                        base,
                        index: src,
                        path,
                        mode,
                    };
                }
            }
            Instruction::AssignPath { path, .. }
            | Instruction::AssignOpPath { path, .. }
            | Instruction::IncDecPath { path, .. }
            | Instruction::UnsetPath { path }
                if path_uses.get(path.index()).copied() == Some(1) =>
            {
                if let Some(definition) = function.paths.get_mut(path.index())
                    && definition
                        .steps
                        .iter()
                        .any(|step| matches!(step, phera_bytecode::Step::Index(reg) if *reg == dst))
                {
                    for step in &mut definition.steps {
                        if let phera_bytecode::Step::Index(reg) = step
                            && *reg == dst
                        {
                            *reg = src;
                        }
                    }
                    function.instructions[index - 1] = Instruction::Nop;
                }
            }
            _ => {}
        }
    }
}

/// Let a binary operator read variables directly instead of the copies
/// made just before it.
///
/// `$i < $n` is compiled as `Move t1, $i; Move t2, $n; Binary t1 < t2`:
/// each operand is copied when it is evaluated, so that a later operand
/// that assigns the variable (`$a + ($a = 5)`) cannot change an earlier
/// one. When the copies are the instructions right before the operator,
/// with no jump target between, nothing can run in between, and the
/// operator reads the variables (`Binary t1, $i < $n`). The VM's operator
/// warns about an unset variable as the copy did, left operand first.
pub(crate) fn fold_operand_copies(function: &mut Function) {
    let boundaries = block_boundaries(function);
    let locals = function.locals.len();
    for index in 1..function.instructions.len() {
        let Instruction::Binary { op, dst, lhs, rhs } = function.instructions[index] else {
            continue;
        };
        let (mut left, mut right) = (lhs, rhs);
        // At most the two copies of the two operands, nearest first.
        for position in (index.saturating_sub(2)..index).rev() {
            if boundaries.contains(&(position + 1)) {
                break;
            }
            let Instruction::Move { dst: copy, src } = function.instructions[position] else {
                break;
            };
            if copy.index() < locals
                || src.index() >= locals
                || (copy != left && copy != right)
                || (copy != dst && !dead_after(function, index, copy))
            {
                break;
            }
            if left == copy {
                left = src;
            }
            if right == copy {
                right = src;
            }
            function.instructions[position] = Instruction::Nop;
        }
        function.instructions[index] = Instruction::Binary {
            op,
            dst,
            lhs: left,
            rhs: right,
        };
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
