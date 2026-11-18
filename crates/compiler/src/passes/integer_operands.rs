//! Fuse adjacent literal operands without moving reads across PHP side effects.
//!
//! Input is lowered register bytecode. Output retains instruction offsets until
//! the normal compaction pass removes Nops and remaps branch targets. Only a
//! neighbouring local read and literal load may disappear: `$a + ($a = 5)` and
//! `$i + $i++` therefore retain their snapshots and evaluation order.
//!
//! For example, LoadInt(r2, 1); Binary(Add, r1, r0, r2) can become
//! Nop; BinaryInt(Add, r1, r0, 1) when r2 is dead after the operation.

use std::collections::HashSet;

use phera_bytecode::{Function, Instruction};

use crate::passes::optimize::target;

/// Replace literal loads used only by the next binary instruction.
pub(crate) fn fuse_integer_operands(function: &mut Function) {
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
        let Instruction::LoadInt {
            dst: literal,
            value,
        } = function.instructions[index - 1]
        else {
            continue;
        };
        let Instruction::Binary { op, dst, lhs, rhs } = function.instructions[index] else {
            continue;
        };
        // The literal slot must be a compiler temporary consumed by this result.
        // Retain its load: later instructions may reuse that same literal slot.
        // A separate liveness check below proves no subsequent read before write.
        if literal != rhs || literal.index() < function.locals.len() {
            continue;
        }
        if !dead_after(function, index, literal) {
            continue;
        }
        let mut source = lhs;
        if index >= 2
            && !boundaries.contains(&(index - 1))
            && let Instruction::Move { dst: snapshot, src } = function.instructions[index - 2]
            && snapshot == lhs
            && snapshot == dst
            && src.index() < function.locals.len()
        {
            source = src;
            function.instructions[index - 2] = Instruction::Nop;
        }
        function.instructions[index - 1] = Instruction::Nop;
        function.instructions[index] = Instruction::BinaryInt {
            op,
            dst,
            lhs: source,
            rhs: value,
        };
    }
}

/// How many instructions [`dead_after`] looks at before it gives up. Each
/// query walks forward until the register is overwritten, so without a bound
/// a generated function with thousands of `case` arms takes quadratic time;
/// giving up only means an operand is not fused.
const DEAD_AFTER_SCAN: usize = 256;

/// A deliberately conservative proof: only straight-line overwrites are accepted.
pub(crate) fn dead_after(function: &Function, index: usize, register: phera_bytecode::Reg) -> bool {
    let mut pending = vec![index + 1];
    // Sorted; at most DEAD_AFTER_SCAN entries, so searching it is cheaper
    // than hashing every position.
    let mut visited: Vec<usize> = Vec::new();
    while let Some(mut cursor) = pending.pop() {
        while let Some(instruction) = function.instructions.get(cursor) {
            match visited.binary_search(&cursor) {
                Ok(_) => break,
                Err(slot) => visited.insert(slot, cursor),
            }
            if visited.len() > DEAD_AFTER_SCAN {
                return false;
            }
            cursor += 1;
            match *instruction {
                Instruction::LoadInt { dst, .. }
                | Instruction::LoadConst { dst, .. }
                | Instruction::LoadNull { dst }
                | Instruction::LoadBool { dst, .. } => {
                    if dst == register {
                        break;
                    }
                }
                Instruction::Clear { reg } => {
                    if reg == register {
                        break;
                    }
                }
                Instruction::Binary { dst, lhs, rhs, .. } => {
                    if lhs == register || rhs == register {
                        return false;
                    }
                    if dst == register {
                        break;
                    }
                }
                Instruction::BinaryInt { dst, lhs, .. } => {
                    if lhs == register {
                        return false;
                    }
                    if dst == register {
                        break;
                    }
                }
                Instruction::Move { dst, src } => {
                    if src == register {
                        return false;
                    }
                    if dst == register {
                        break;
                    }
                }
                Instruction::IncDecLocal { dst, local, .. } => {
                    if local == register {
                        return false;
                    }
                    if dst == register {
                        break;
                    }
                }
                Instruction::Assign { src, local } => {
                    if src == register {
                        return false;
                    }
                    if local == register {
                        break;
                    }
                }
                Instruction::Echo { src } | Instruction::SendValue { src } => {
                    if src == register {
                        return false;
                    }
                }
                Instruction::AssignOp { src, local, .. } => {
                    if src == register || local == register {
                        return false;
                    }
                }
                Instruction::Jump { target } => cursor = target.index(),
                Instruction::JumpIf { condition, target }
                | Instruction::JumpIfNot { condition, target } => {
                    if condition == register {
                        return false;
                    }
                    pending.push(target.index());
                }
                Instruction::Return { src } => {
                    if src == register {
                        return false;
                    }
                    break;
                }
                Instruction::InitCall { .. } | Instruction::Nop => {}
                Instruction::Call { dst } => {
                    if dst == register {
                        break;
                    }
                }
                Instruction::CallDirect {
                    args, count, dst, ..
                } => {
                    if (args.0..args.0 + count).contains(&register.0) {
                        return false;
                    }
                    if dst == register {
                        break;
                    }
                }
                Instruction::IterStart { src, .. } => {
                    if src == register {
                        return false;
                    }
                }
                Instruction::IterNextKeyed {
                    key, value, end, ..
                } => {
                    pending.push(end.index());
                    if key == register || value == register {
                        break;
                    }
                }
                Instruction::IterNext { value, end, .. } => {
                    pending.push(end.index());
                    if value == register {
                        break;
                    }
                }
                _ => match crate::passes::object_liveness::effect(function, *instruction, register)
                {
                    crate::passes::object_liveness::RegisterEffect::Read
                    | crate::passes::object_liveness::RegisterEffect::Unknown => return false,
                    crate::passes::object_liveness::RegisterEffect::Write => break,
                    crate::passes::object_liveness::RegisterEffect::Unrelated => {}
                },
            }
        }
    }
    true
}

/// Consume a comparison result immediately instead of storing a temporary bool.
pub(crate) fn fuse_integer_branches(function: &mut Function) {
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
        let Instruction::BinaryInt { op, dst, lhs, rhs } = function.instructions[index - 1] else {
            continue;
        };
        let Instruction::JumpIfNot { condition, target } = function.instructions[index] else {
            continue;
        };
        if condition != dst
            || dst.index() < function.locals.len()
            || !dead_after(function, index, dst)
        {
            continue;
        }
        if !matches!(
            op,
            phera_bytecode::BinaryOp::Less
                | phera_bytecode::BinaryOp::LessEqual
                | phera_bytecode::BinaryOp::Greater
                | phera_bytecode::BinaryOp::GreaterEqual
                | phera_bytecode::BinaryOp::Equal
                | phera_bytecode::BinaryOp::NotEqual
                | phera_bytecode::BinaryOp::Identical
                | phera_bytecode::BinaryOp::NotIdentical
        ) {
            continue;
        }
        function.instructions[index - 1] = Instruction::BinaryIntBranch {
            op,
            lhs,
            rhs,
            target,
        };
        function.instructions[index] = Instruction::Nop;
    }
}
