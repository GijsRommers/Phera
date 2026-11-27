//! Use a local directly when an adjacent consuming instruction cannot change it.
//!
//! Input and output are lowered bytecode; removed snapshots become Nops for the
//! subsequent target remapping pass. No read crosses another PHP expression.
//!
//! Input bytecode retains stable offsets during this pass. Replacements leave
//! Nops for later compaction. For example, Move(r1, r0); Return(r1) becomes
//! Nop; Return(r0) when r0 is a local and r1 a temporary.

use std::collections::VecDeque;

use phera_bytecode::{Function, Instruction, PathBase, Reg};

use crate::passes::{
    integer_operands::dead_after,
    optimize::{block_boundaries, target},
};

/// Remove adjacent local snapshots whose temporary has no later reader, and
/// fold temporary-to-temporary moves immediately before `Return`.
pub(crate) fn optimize(function: &mut Function) {
    let boundaries = block_boundaries(function);
    let locals = function.locals.len();
    let defined = definitely_defined_locals(function);
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
            } if key_reg == dst
                && local_is_defined(defined[index - 1], src)
                && path_uses.get(path.index()).copied() == Some(1) =>
            {
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
                if local_is_defined(defined[index - 1], src)
                    && path_uses.get(path.index()).copied() == Some(1) =>
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

#[inline]
fn local_bit(reg: Reg, tracked: usize) -> u64 {
    let idx = reg.index();
    if idx < tracked { 1u64 << idx } else { 0 }
}

#[inline]
fn local_is_defined(mask: u64, reg: Reg) -> bool {
    let idx = reg.index();
    idx < 64 && (mask & (1u64 << idx)) != 0
}

/// Forward must-defined analysis over the first 64 named locals.
///
/// A bit is set at `ip` only if the corresponding local is guaranteed to hold
/// a defined slot on every control-flow path reaching `ip`, so folding a
/// `Move` of that local into a `Step::Index` cannot suppress an
/// `"Undefined variable"` warning or reorder error handlers.
fn definitely_defined_locals(function: &Function) -> Vec<u64> {
    let len = function.instructions.len();
    let tracked = function.locals.len().min(64);
    if len == 0 || tracked == 0 {
        return vec![0; len];
    }

    let mut init_mask = 0u64;
    for (idx, param) in function.parameters.iter().enumerate() {
        if idx < tracked && !param.has_default {
            init_mask |= 1u64 << idx;
        }
    }

    let mut entry: Vec<Option<u64>> = vec![None; len];
    let mut worklist = VecDeque::new();
    let push_edge =
        |dst: usize, mask: u64, entry: &mut [Option<u64>], worklist: &mut VecDeque<usize>| {
            if dst >= entry.len() {
                return;
            }
            match entry[dst] {
                None => {
                    entry[dst] = Some(mask);
                    worklist.push_back(dst);
                }
                Some(old) => {
                    let merged = old & mask;
                    if merged != old {
                        entry[dst] = Some(merged);
                        worklist.push_back(dst);
                    }
                }
            }
        };

    push_edge(0, init_mask, &mut entry, &mut worklist);
    for handler in &function.handlers {
        if let Some(catch) = handler.catch {
            push_edge(catch.index(), 0, &mut entry, &mut worklist);
        }
        if let Some(finally) = handler.finally {
            push_edge(finally.index(), 0, &mut entry, &mut worklist);
        }
    }

    while let Some(ip) = worklist.pop_front() {
        let mask = entry[ip].unwrap_or(0);
        let instruction = function.instructions[ip];

        let mut fallthrough_mask = mask;
        let mut taken_mask = mask;
        let mut falls_through = true;

        match instruction {
            Instruction::Assign { local, .. }
            | Instruction::AssignConst { local, .. }
            | Instruction::AssignTake { local, .. }
            | Instruction::AssignOp { local, .. }
            | Instruction::IncDecLocal { local, .. }
            | Instruction::BindGlobal { local, .. }
            | Instruction::BindStatic { local, .. } => {
                fallthrough_mask |= local_bit(local, tracked);
            }
            Instruction::UnsetLocal { local } => {
                fallthrough_mask &= !local_bit(local, tracked);
            }
            Instruction::ReceiveDefault { param, .. } => {
                taken_mask |= local_bit(param, tracked);
            }
            Instruction::IterNext { value, .. } => {
                fallthrough_mask |= local_bit(value, tracked);
            }
            Instruction::IterNextKeyed { key, value, .. } => {
                fallthrough_mask |= local_bit(key, tracked) | local_bit(value, tracked);
            }
            Instruction::AssignPath { path, .. }
            | Instruction::AssignPathFromLocal { path, .. }
            | Instruction::AssignOpPath { path, .. }
            | Instruction::IncDecPath { path, .. }
            | Instruction::BindReference { path, .. }
            | Instruction::MakeReference { path, .. } => {
                if let Some(def) = function.paths.get(path.index())
                    && def.steps.is_empty()
                    && let PathBase::Register(local) = def.base
                {
                    fallthrough_mask |= local_bit(local, tracked);
                }
            }
            Instruction::UnsetPath { path } => {
                if let Some(def) = function.paths.get(path.index()) {
                    if def.steps.is_empty()
                        && let PathBase::Register(local) = def.base
                    {
                        fallthrough_mask &= !local_bit(local, tracked);
                    } else if function.dynamic_scope && !matches!(def.base, PathBase::Register(_)) {
                        fallthrough_mask = 0;
                    }
                }
            }
            Instruction::Jump { .. }
            | Instruction::Return { .. }
            | Instruction::ReturnNull
            | Instruction::ReturnChecked { .. }
            | Instruction::GeneratorReturn { .. }
            | Instruction::Throw { .. }
            | Instruction::FinallyReturn { .. } => {
                falls_through = false;
            }
            Instruction::Call { .. }
            | Instruction::CallDirect { .. }
            | Instruction::CallMethod { .. }
            | Instruction::CallNativeDirect { .. }
            | Instruction::Eval { .. }
            | Instruction::Include { .. }
            | Instruction::BindDynamicGlobal { .. }
                if function.dynamic_scope =>
            {
                fallthrough_mask = 0;
                taken_mask = 0;
            }
            _ => {}
        }

        if falls_through {
            push_edge(ip + 1, fallthrough_mask, &mut entry, &mut worklist);
        }
        if let Some(dst) = target(&instruction) {
            push_edge(dst.index(), taken_mask, &mut entry, &mut worklist);
        }
    }

    entry.into_iter().map(|mask| mask.unwrap_or(0)).collect()
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
