//! Lazy bytecode -> Tier-2 [`FastFunction`] lowerer with:
//! - Static clean-loop / self-recursion eligibility check (eliminates deopt
//!   thrashing on OOP, native, and HTTP workloads)
//! - Float-type-hint propagation (emits float-first `AddF64`/`SubF64`/`MulF64`
//!   ops for float-dominated loops like `mandel`/`mandel2`/`heapsort`)
//! - General peephole fusions (`IncLoop*`, `DecLoop*`, `RetIntImm`,
//!   `ArrAddFromArr`, `CallSelf1`, `CallSelf2`, `CallNoop`, producer+`Assign`)

use std::sync::Arc;
use std::sync::atomic::AtomicU8;

use phera_bytecode::{
    BinaryOp, BuiltinUnary, CastKind, CompiledUnit, Constant, FetchMode, Function, IncDec,
    Instruction as I, PathBase, Reg, Step, Target, UnaryOp,
};

use crate::code_table::{CodeTable, FunctionIndex, FunctionShape};
use crate::fast_tier::{
    CMP_EQ, CMP_GE, CMP_GT, CMP_IDENT, CMP_LE, CMP_LT, CMP_NE, CMP_NOT_IDENT, FastFunction, FastOp,
    Op,
};

const MAX_FAST_REGS: usize = 255;
const MAX_FAST_OPS: usize = 4096;

/// Fast static check run once at `CodeTable::load` time (zero runtime cost!).
///
/// A function is a Tier-2 candidate only if it contains:
/// 1. At least one **Tier-2-clean loop** (a backward branch `target..=ip` where
///    every instruction inside the loop body is natively handled by Tier 2,
///    preventing deopt thrashing on OOP/native/foreach loops), OR
/// 2. Direct self-recursion (`CallDirect` to itself) with no unhandled ops.
pub(crate) fn is_fast_candidate(function: &Function, self_local: Option<u32>) -> bool {
    if (function.registers as usize) > MAX_FAST_REGS
        || function.instructions.is_empty()
        || function.instructions.len() > MAX_FAST_OPS
        || !function.handlers.is_empty()
        || function.is_generator
        || function.returns_reference
        || function.observes_arguments
    {
        return false;
    }

    let insts = &function.instructions;

    // 1. Check for any Tier-2-clean loop body `target..=ip`.
    for (ip, &inst) in insts.iter().enumerate() {
        if let Some(target) = jump_target(inst) {
            let start = target.index();
            if start <= ip
                && insts[start..=ip]
                    .iter()
                    .all(|&op| is_supported_in_loop(function, op))
            {
                return true;
            }
        }
    }

    // 2. Check for self-recursive plain functions (e.g. `fibo_r`, `Ack`).
    if let Some(local_id) = self_local {
        let mut calls_self = false;
        let mut has_unsupported = false;
        for &inst in insts.iter() {
            if let I::CallDirect {
                function: declared,
                count,
                ..
            } = inst
                && declared.0 == local_id
                && count <= 16
            {
                calls_self = true;
            }
            if !is_supported_in_loop(function, inst) {
                has_unsupported = true;
                break;
            }
        }
        if calls_self && !has_unsupported {
            return true;
        }
    }

    false
}

/// Whether `inst` executes natively inside Tier 2 without deopting.
fn is_supported_in_loop(function: &Function, inst: I) -> bool {
    match inst {
        I::Nop
        | I::LoadInt { .. }
        | I::LoadBool { .. }
        | I::LoadNull { .. }
        | I::LoadConst { .. }
        | I::AssignConst { .. }
        | I::NewArray { .. }
        | I::Move { .. }
        | I::Assign { .. }
        | I::AssignTake { .. }
        | I::Clear { .. }
        | I::IncDecLocal { .. }
        | I::Jump { .. }
        | I::JumpIf { .. }
        | I::JumpIfNot { .. }
        | I::JumpIfNull { .. }
        | I::JumpIfNotNull { .. }
        | I::AppendLocal { .. }
        | I::Return { .. }
        | I::ReturnNull => true,
        I::Binary { op, .. } => lower_binary(op, 0, 0, 0, false).is_some(),
        I::BinaryInt { op, .. } => lower_binary_int(op, 0, 0, 0, false).is_some(),
        I::AssignOp { op, local, src } => {
            matches!(op, BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply)
                || (op == BinaryOp::Concat && local != src)
        }
        I::Unary { op, .. } => matches!(op, UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitwiseNot),
        I::Cast { to, .. } => matches!(to, CastKind::Int | CastKind::Float | CastKind::Bool),
        I::BuiltinUnary {
            op: BuiltinUnary::Strlen,
            ..
        } => true,
        I::BinaryBranch { op, .. } | I::BinaryBranchIf { op, .. } => cmp_code(op).is_some(),
        I::BinaryIntBranch { op, .. } | I::BinaryIntBranchIf { op, .. } => {
            branch_imm_op(op).is_some()
        }
        I::FetchIndex { mode, .. } => mode != FetchMode::Destructure,
        I::Fetch { path, mode, .. } => lower_fetch_2d(function, 0, path, mode).is_some(),
        I::AssignPath { path, .. } | I::UnsetPath { path } => {
            function.paths.get(path.index()).is_some_and(|def| {
                matches!(def.base, PathBase::Register(_))
                    && matches!(def.steps.as_slice(), [Step::Index(_)])
            })
        }
        I::AssignOpPath {
            op: BinaryOp::Add,
            path,
            ..
        } => function.paths.get(path.index()).is_some_and(|def| {
            matches!(def.base, PathBase::Register(_))
                && matches!(def.steps.as_slice(), [Step::Index(_)])
        }),
        I::CallDirect { count, .. } => count <= 16,
        _ => false,
    }
}

/// Lower `function_idx` into a [`FastFunction`], returning any `CallDirect`
/// callees to transitively compile.
pub(crate) fn lower_function(
    code: &CodeTable,
    function_idx: FunctionIndex,
    require_candidate: bool,
) -> Option<(Arc<FastFunction>, Vec<FunctionIndex>)> {
    let (function, _cache, facts) = code.function_parts(function_idx);
    if require_candidate && !facts.fast_candidate {
        return None;
    }
    if (function.registers as usize) > MAX_FAST_REGS
        || function.instructions.is_empty()
        || function.instructions.len() > MAX_FAST_OPS
        || !function.handlers.is_empty()
        || function.is_generator
        || function.returns_reference
        || function.observes_arguments
    {
        return None;
    }
    let unit = code.unit(function_idx);
    let (fast, direct_callees) = lower_body(code, function_idx, function, unit, facts)?;
    Some((Arc::new(fast), direct_callees))
}

fn lower_body(
    code: &CodeTable,
    self_idx: FunctionIndex,
    function: &Function,
    unit: &CompiledUnit,
    facts: &crate::function_facts::FunctionFacts,
) -> Option<(FastFunction, Vec<FunctionIndex>)> {
    let instructions = &function.instructions;
    let len = instructions.len();
    let locals = function.locals.len();

    // Mark jump targets so peephole fusions never cross a control-flow boundary.
    let mut is_target = vec![false; len + 1];
    for inst in instructions.iter() {
        if let Some(t) = jump_target(*inst)
            && t.index() < is_target.len()
        {
            is_target[t.index()] = true;
        }
    }

    // Count reads per register so we know when a temporary is unused or single-use.
    let mut reads = vec![0u32; function.registers as usize];
    for inst in instructions.iter() {
        count_reads(function, *inst, &mut reads);
    }
    let _ = &reads;

    // Simple 2-pass float-hint propagation so float-dominated loops (`mandel`,
    // `mandel2`, `heapsort`) emit float-first `AddF64`/`SubF64`/`MulF64` ops!
    let float_regs = infer_float_hints(function, unit);

    let mut ops: Vec<FastOp> = Vec::with_capacity(len);
    let mut bc_ip: Vec<u32> = Vec::with_capacity(len);
    let mut bc_to_fast: Vec<u16> = vec![u16::MAX; len];
    let mut f64_consts: Vec<f64> = Vec::new();
    let mut direct_callees: Vec<FunctionIndex> = Vec::new();
    let mut patch_jumps: Vec<(usize, usize, JumpPatch)> = Vec::new();

    let self_fast_recursive = facts.shape == FunctionShape::Plain
        && facts.takes_values_only()
        && facts.typed_parameters() == 0
        && facts.return_mask.is_none();

    let mut i = 0usize;
    while i < len {
        let fast_idx = ops.len();
        if fast_idx >= 32_767 {
            return None;
        }
        bc_to_fast[i] = fast_idx as u16;
        let inst = instructions[i];
        let next = (i + 1 < len && !is_target[i + 1]).then(|| instructions[i + 1]);

        // --- Fusion 1: `LoadInt { dst: t, value }; Return { src: t }` -> `RetIntImm` ---
        if let (I::LoadInt { dst, value }, Some(I::Return { src })) = (inst, next)
            && dst == src
            && dst.index() >= locals
        {
            ops.push(FastOp::new(Op::RetIntImm, 0, 0, 0, value));
            bc_ip.push(i as u32);
            i += 2;
            continue;
        }

        // --- Fusion 1b: `LoadInt(t, !=0) / LoadBool(t, true) + JumpIf(t, target)` -> `Jump(target)` ---
        if let Some(I::JumpIf { condition, target }) = next
            && condition.index() >= locals
            && temp_dead_after(instructions, &is_target, i + 1, condition, function)
            && match inst {
                I::LoadInt { dst, value } => dst == condition && value != 0,
                I::LoadBool { dst, value } => dst == condition && value,
                _ => false,
            }
        {
            let op_pos = ops.len();
            ops.push(FastOp::new(Op::Jump, 0, 0, 0, 0));
            bc_ip.push(i as u32);
            patch_jumps.push((op_pos, target.index(), JumpPatch::Imm));
            i += 2;
            continue;
        }

        // --- Fusion 1c: `LoadInt(t, imm) + Binary(Add/Mul, dst, t, rhs)` -> `AddImm/MulImm/AddF64Imm/MulF64Imm` ---
        if let (I::LoadInt { dst: ld_dst, value }, Some(I::Binary { op, dst, lhs, rhs })) =
            (inst, next)
            && matches!(op, BinaryOp::Add | BinaryOp::Multiply)
            && ld_dst.index() >= locals
            && (lhs == ld_dst || rhs == ld_dst)
            && (dst == ld_dst
                || reads[ld_dst.index()] == 1
                || temp_dead_after(instructions, &is_target, i + 1, ld_dst, function))
        {
            let other = if lhs == ld_dst { rhs } else { lhs };
            let flt = float_regs[other.index()];
            if let Some(fast) = lower_binary_int(op, dst.0 as u8, other.0 as u8, value, flt) {
                ops.push(fast);
                bc_ip.push(i as u32);
                i += 2;
                continue;
            }
        }

        // --- Fusion 2: `FetchIndex(t, X, j) + AssignOpPath(Add, dead, Y[j], t)` -> `ArrAddFromArr` ---
        if let (
            I::FetchIndex {
                dst: fetch_dst,
                base: src_arr,
                index: src_idx,
                mode: FetchMode::Read,
                ..
            },
            Some(I::AssignOpPath {
                op: BinaryOp::Add,
                dst: assign_dst,
                path,
                src,
            }),
        ) = (inst, next)
            && fetch_dst == src
            && fetch_dst.index() >= locals
            && temp_dead_after(instructions, &is_target, i + 1, fetch_dst, function)
            && assign_dst.index() >= locals
            && temp_dead_after(instructions, &is_target, i + 1, assign_dst, function)
            && let Some(def) = function.paths.get(path.index())
            && let PathBase::Register(dst_arr) = def.base
            && let [Step::Index(dst_idx)] = def.steps.as_slice()
            && *dst_idx == src_idx
        {
            ops.push(FastOp::new(
                Op::ArrAddFromArr,
                dst_arr.0 as u8,
                src_arr.0 as u8,
                src_idx.0 as u8,
                0,
            ));
            bc_ip.push(i as u32);
            i += 2;
            continue;
        }

        // --- Fusion 2b: `Binary(Multiply, t, lhs, rhs) + AssignOp(Add, acc, t)` -> `MulAddAssign` ---
        if let (
            I::Binary {
                op: BinaryOp::Multiply,
                dst: mul_dst,
                lhs,
                rhs,
            },
            Some(I::AssignOp {
                op: BinaryOp::Add,
                local: acc,
                src,
            }),
        ) = (inst, next)
            && mul_dst == src
            && mul_dst.index() >= locals
            && temp_dead_after(instructions, &is_target, i + 1, mul_dst, function)
        {
            ops.push(FastOp::new(
                Op::MulAddAssign,
                acc.0 as u8,
                lhs.0 as u8,
                rhs.0 as u8,
                0,
            ));
            bc_ip.push(i as u32);
            i += 2;
            continue;
        }

        // --- Fusion 3: Rotated loop latch `IncDecLocal(dead dst) + Binary*BranchIf` ---
        if let I::IncDecLocal { mode, dst, local } = inst
            && dst.index() >= locals
            && temp_dead_after(instructions, &is_target, i, dst, function)
        {
            let is_inc = matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement);
            match next {
                Some(I::BinaryIntBranchIf {
                    op,
                    lhs,
                    rhs,
                    target,
                }) if lhs == local => {
                    let fused = match (is_inc, op) {
                        (true, BinaryOp::Less) => Some(Op::IncLoopLtImm),
                        (true, BinaryOp::LessEqual) => Some(Op::IncLoopLeImm),
                        (false, BinaryOp::GreaterEqual) => Some(Op::DecLoopGeImm),
                        (false, BinaryOp::Greater) => Some(Op::DecLoopGtImm),
                        _ => None,
                    };
                    if let Some(fused_op) = fused {
                        let op_pos = ops.len();
                        ops.push(FastOp::new(fused_op, local.0 as u8, 0, 0, rhs));
                        bc_ip.push(i as u32);
                        patch_jumps.push((op_pos, target.index(), JumpPatch::Bc16));
                        i += 2;
                        continue;
                    }
                }
                Some(I::BinaryBranchIf {
                    op,
                    lhs,
                    rhs,
                    target,
                }) if lhs == local => {
                    let fused = match (is_inc, op) {
                        (true, BinaryOp::Less) => Some(Op::IncLoopLtReg),
                        (true, BinaryOp::LessEqual) => Some(Op::IncLoopLeReg),
                        (false, BinaryOp::GreaterEqual) => Some(Op::DecLoopGeReg),
                        (false, BinaryOp::Greater) => Some(Op::DecLoopGtReg),
                        _ => None,
                    };
                    if let Some(fused_op) = fused {
                        let op_pos = ops.len();
                        ops.push(FastOp::new(fused_op, local.0 as u8, rhs.0 as u8, 0, 0));
                        bc_ip.push(i as u32);
                        patch_jumps.push((op_pos, target.index(), JumpPatch::Imm));
                        i += 2;
                        continue;
                    }
                }
                _ => {}
            }
            let fast_op = if is_inc {
                Op::IncLocalFast
            } else {
                Op::DecLocalFast
            };
            ops.push(FastOp::new(fast_op, local.0 as u8, 0, 0, 0));
            bc_ip.push(i as u32);
            i += 1;
            continue;
        }

        // --- Fusion 4: Single-use temp producer + `Assign` / `AssignTake` ---
        let fused_assign_dst = match next {
            Some(I::Assign { local, src } | I::AssignTake { local, src })
                if src.index() >= locals
                    && (reads[src.index()] == 1
                        || temp_dead_after(instructions, &is_target, i + 1, src, function)) =>
            {
                Some((local, src))
            }
            _ => None,
        };

        if let Some((assign_local, assign_src)) = fused_assign_dst
            && let Some(fused) = try_lower_producer(
                inst,
                assign_src,
                assign_local,
                function,
                unit,
                &float_regs,
                &mut f64_consts,
            )
        {
            ops.push(fused);
            bc_ip.push(i as u32);
            i += 2;
            continue;
        }

        // --- Standard single-instruction lowering ---
        match inst {
            I::Nop => {
                bc_to_fast[i] = ops.len() as u16;
            }
            I::LoadInt { dst, value } => {
                ops.push(FastOp::new(Op::LoadInt, dst.0 as u8, 0, 0, value));
                bc_ip.push(i as u32);
            }
            I::LoadBool { dst, value } => {
                ops.push(FastOp::new(
                    Op::LoadBool,
                    dst.0 as u8,
                    u8::from(value),
                    0,
                    0,
                ));
                bc_ip.push(i as u32);
            }
            I::LoadNull { dst } => {
                ops.push(FastOp::new(Op::LoadNull, dst.0 as u8, 0, 0, 0));
                bc_ip.push(i as u32);
            }
            I::LoadConst { dst, constant } => {
                ops.push(lower_load_const(
                    dst.0 as u8,
                    constant.0,
                    unit,
                    &mut f64_consts,
                ));
                bc_ip.push(i as u32);
            }
            I::AssignConst { local, constant } => {
                ops.push(lower_load_const(
                    local.0 as u8,
                    constant.0,
                    unit,
                    &mut f64_consts,
                ));
                bc_ip.push(i as u32);
            }
            I::NewArray { dst, capacity } => {
                ops.push(FastOp::new(
                    Op::NewArray,
                    dst.0 as u8,
                    0,
                    0,
                    i32::from(capacity),
                ));
                bc_ip.push(i as u32);
            }
            I::Move { dst, src } => {
                ops.push(FastOp::new(Op::Move, dst.0 as u8, src.0 as u8, 0, 0));
                bc_ip.push(i as u32);
            }
            I::Assign { local, src } => {
                ops.push(FastOp::new(Op::Assign, local.0 as u8, src.0 as u8, 0, 0));
                bc_ip.push(i as u32);
            }
            I::AssignTake { local, src } => {
                ops.push(FastOp::new(
                    Op::AssignTake,
                    local.0 as u8,
                    src.0 as u8,
                    0,
                    0,
                ));
                bc_ip.push(i as u32);
            }
            I::Clear { reg } => {
                ops.push(FastOp::new(Op::Clear, reg.0 as u8, 0, 0, 0));
                bc_ip.push(i as u32);
            }
            I::Binary { op, dst, lhs, rhs } => {
                let flt = float_regs[lhs.index()] || float_regs[rhs.index()];
                if let Some(fast) = lower_binary(op, dst.0 as u8, lhs.0 as u8, rhs.0 as u8, flt) {
                    ops.push(fast);
                } else {
                    ops.push(FastOp::new(Op::Deopt, 0, 0, 0, 0));
                }
                bc_ip.push(i as u32);
            }
            I::BinaryInt { op, dst, lhs, rhs } => {
                let flt = float_regs[lhs.index()];
                if let Some(fast) = lower_binary_int(op, dst.0 as u8, lhs.0 as u8, rhs, flt) {
                    ops.push(fast);
                } else {
                    ops.push(FastOp::new(Op::Deopt, 0, 0, 0, 0));
                }
                bc_ip.push(i as u32);
            }
            I::AssignOp { op, local, src } => {
                let fast = match op {
                    BinaryOp::Add => FastOp::new(Op::AddAssign, local.0 as u8, src.0 as u8, 0, 0),
                    BinaryOp::Subtract => {
                        FastOp::new(Op::SubAssign, local.0 as u8, src.0 as u8, 0, 0)
                    }
                    BinaryOp::Multiply => {
                        FastOp::new(Op::MulAssign, local.0 as u8, src.0 as u8, 0, 0)
                    }
                    BinaryOp::Concat if local != src => {
                        FastOp::new(Op::StrAppend, local.0 as u8, src.0 as u8, 0, 0)
                    }
                    _ => FastOp::new(Op::Deopt, 0, 0, 0, 0),
                };
                ops.push(fast);
                bc_ip.push(i as u32);
            }
            I::Unary { op, dst, src } => {
                let fast = match op {
                    UnaryOp::Negate => FastOp::new(Op::NegNum, dst.0 as u8, src.0 as u8, 0, 0),
                    UnaryOp::Not => FastOp::new(Op::NotBool, dst.0 as u8, src.0 as u8, 0, 0),
                    UnaryOp::BitwiseNot => {
                        FastOp::new(Op::BitNotInt, dst.0 as u8, src.0 as u8, 0, 0)
                    }
                    UnaryOp::Plus => FastOp::new(Op::Deopt, 0, 0, 0, 0),
                };
                ops.push(fast);
                bc_ip.push(i as u32);
            }
            I::Cast { to, dst, src } => {
                let fast = match to {
                    CastKind::Int => FastOp::new(Op::CastInt, dst.0 as u8, src.0 as u8, 0, 0),
                    CastKind::Float => FastOp::new(Op::CastFloat, dst.0 as u8, src.0 as u8, 0, 0),
                    CastKind::Bool => FastOp::new(Op::CastBool, dst.0 as u8, src.0 as u8, 0, 0),
                    _ => FastOp::new(Op::Deopt, 0, 0, 0, 0),
                };
                ops.push(fast);
                bc_ip.push(i as u32);
            }
            I::BuiltinUnary {
                op: BuiltinUnary::Strlen,
                dst,
                src,
            } => {
                ops.push(FastOp::new(Op::StrLen, dst.0 as u8, src.0 as u8, 0, 0));
                bc_ip.push(i as u32);
            }
            I::IncDecLocal { mode, dst, local } => {
                let mode_u8 = match mode {
                    IncDec::PreIncrement => 0,
                    IncDec::PostIncrement => 1,
                    IncDec::PreDecrement => 2,
                    IncDec::PostDecrement => 3,
                };
                ops.push(FastOp::new(
                    Op::IncDecLocal,
                    dst.0 as u8,
                    local.0 as u8,
                    mode_u8,
                    0,
                ));
                bc_ip.push(i as u32);
            }
            I::Jump { target } => {
                let op_pos = ops.len();
                ops.push(FastOp::new(Op::Jump, 0, 0, 0, 0));
                bc_ip.push(i as u32);
                patch_jumps.push((op_pos, target.index(), JumpPatch::Imm));
            }
            I::JumpIf { condition, target } => {
                let op_pos = ops.len();
                ops.push(FastOp::new(Op::JumpIf, condition.0 as u8, 0, 0, 0));
                bc_ip.push(i as u32);
                patch_jumps.push((op_pos, target.index(), JumpPatch::Imm));
            }
            I::JumpIfNot { condition, target } => {
                let op_pos = ops.len();
                ops.push(FastOp::new(Op::JumpIfNot, condition.0 as u8, 0, 0, 0));
                bc_ip.push(i as u32);
                patch_jumps.push((op_pos, target.index(), JumpPatch::Imm));
            }
            I::JumpIfNull { value, target } => {
                let op_pos = ops.len();
                ops.push(FastOp::new(Op::JumpIfNull, value.0 as u8, 0, 0, 0));
                bc_ip.push(i as u32);
                patch_jumps.push((op_pos, target.index(), JumpPatch::Imm));
            }
            I::JumpIfNotNull { value, target } => {
                let op_pos = ops.len();
                ops.push(FastOp::new(Op::JumpIfNotNull, value.0 as u8, 0, 0, 0));
                bc_ip.push(i as u32);
                patch_jumps.push((op_pos, target.index(), JumpPatch::Imm));
            }
            I::BinaryBranch {
                op,
                lhs,
                rhs,
                target,
            } => {
                if let Some(cmp) = cmp_code(op) {
                    let op_pos = ops.len();
                    ops.push(FastOp::new(Op::BranchCmp, cmp, lhs.0 as u8, rhs.0 as u8, 0));
                    bc_ip.push(i as u32);
                    patch_jumps.push((op_pos, target.index(), JumpPatch::Imm));
                } else {
                    ops.push(FastOp::new(Op::Deopt, 0, 0, 0, 0));
                    bc_ip.push(i as u32);
                }
            }
            I::BinaryBranchIf {
                op,
                lhs,
                rhs,
                target,
            } => {
                if let Some(cmp) = cmp_code(op) {
                    let op_pos = ops.len();
                    ops.push(FastOp::new(
                        Op::BranchCmp,
                        cmp | 0x10,
                        lhs.0 as u8,
                        rhs.0 as u8,
                        0,
                    ));
                    bc_ip.push(i as u32);
                    patch_jumps.push((op_pos, target.index(), JumpPatch::Imm));
                } else {
                    ops.push(FastOp::new(Op::Deopt, 0, 0, 0, 0));
                    bc_ip.push(i as u32);
                }
            }
            I::BinaryIntBranch {
                op,
                lhs,
                rhs,
                target,
            } => {
                if let Some(bop) = branch_imm_op(op) {
                    let op_pos = ops.len();
                    ops.push(FastOp::new(bop, lhs.0 as u8, 0, 0, rhs));
                    bc_ip.push(i as u32);
                    patch_jumps.push((op_pos, target.index(), JumpPatch::Bc15When(false)));
                } else {
                    ops.push(FastOp::new(Op::Deopt, 0, 0, 0, 0));
                    bc_ip.push(i as u32);
                }
            }
            I::BinaryIntBranchIf {
                op,
                lhs,
                rhs,
                target,
            } => {
                if let Some(bop) = branch_imm_op(op) {
                    let op_pos = ops.len();
                    ops.push(FastOp::new(bop, lhs.0 as u8, 0, 0, rhs));
                    bc_ip.push(i as u32);
                    patch_jumps.push((op_pos, target.index(), JumpPatch::Bc15When(true)));
                } else {
                    ops.push(FastOp::new(Op::Deopt, 0, 0, 0, 0));
                    bc_ip.push(i as u32);
                }
            }
            I::FetchIndex {
                dst,
                base,
                index,
                mode,
                ..
            } => {
                let quiet = matches!(mode, FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty);
                ops.push(FastOp::new(
                    Op::ArrGetInt,
                    dst.0 as u8,
                    base.0 as u8,
                    index.0 as u8,
                    i32::from(quiet),
                ));
                bc_ip.push(i as u32);
            }
            I::Fetch { dst, path, mode } => {
                if let Some(fast) = lower_fetch_2d(function, dst.0 as u8, path, mode) {
                    ops.push(fast);
                } else {
                    ops.push(FastOp::new(Op::Deopt, 0, 0, 0, 0));
                }
                bc_ip.push(i as u32);
            }
            I::AssignPath { path, src } => {
                if let Some(def) = function.paths.get(path.index())
                    && let PathBase::Register(arr) = def.base
                    && let [Step::Index(idx)] = def.steps.as_slice()
                {
                    ops.push(FastOp::new(
                        Op::ArrSetInt,
                        arr.0 as u8,
                        idx.0 as u8,
                        src.0 as u8,
                        0,
                    ));
                } else {
                    ops.push(FastOp::new(Op::Deopt, 0, 0, 0, 0));
                }
                bc_ip.push(i as u32);
            }
            I::AssignOpPath {
                op: BinaryOp::Add,
                dst,
                path,
                src,
            } if dst.index() >= locals && reads[dst.index()] == 0 => {
                if let Some(def) = function.paths.get(path.index())
                    && let PathBase::Register(arr) = def.base
                    && let [Step::Index(idx)] = def.steps.as_slice()
                {
                    ops.push(FastOp::new(
                        Op::ArrAddAssignInt,
                        arr.0 as u8,
                        idx.0 as u8,
                        src.0 as u8,
                        0,
                    ));
                } else {
                    ops.push(FastOp::new(Op::Deopt, 0, 0, 0, 0));
                }
                bc_ip.push(i as u32);
            }
            I::AppendLocal { local, src } => {
                ops.push(FastOp::new(Op::ArrPush, local.0 as u8, src.0 as u8, 0, 0));
                bc_ip.push(i as u32);
            }
            I::UnsetPath { path } => {
                if let Some(def) = function.paths.get(path.index())
                    && let PathBase::Register(arr) = def.base
                    && let [Step::Index(idx)] = def.steps.as_slice()
                {
                    ops.push(FastOp::new(Op::ArrUnsetInt, arr.0 as u8, idx.0 as u8, 0, 0));
                } else {
                    ops.push(FastOp::new(Op::Deopt, 0, 0, 0, 0));
                }
                bc_ip.push(i as u32);
            }
            I::CallDirect {
                function: declared,
                args,
                count,
                dst,
                ..
            } if count <= 16 => {
                let callee_idx = FunctionIndex(facts.unit_first.0 + 1 + declared.0);
                // Specialize self-recursive 1-arg and 2-arg untyped plain calls (`fibo_r`, `Ack`).
                if callee_idx == self_idx
                    && self_fast_recursive
                    && count == facts.parameters
                    && (count == 1 || count == 2)
                {
                    let op = if count == 1 {
                        Op::CallSelf1
                    } else {
                        Op::CallSelf2
                    };
                    ops.push(FastOp::new(
                        op,
                        dst.0 as u8,
                        args.0 as u8,
                        (args.0 + 1) as u8,
                        0,
                    ));
                } else if let Some((_, callee_facts)) =
                    code.declared_facts(facts.unit_first, declared.index())
                    && callee_facts.shape == FunctionShape::Plain
                    && callee_facts.takes_values_only()
                    && callee_facts.typed_parameters() == 0
                    && count == callee_facts.parameters
                    && callee_facts.return_mask.is_none_or(|m| {
                        m.intersects(phera_bytecode::TypeMask::NULL)
                            || m.intersects(phera_bytecode::TypeMask::VOID)
                    })
                    && callee_facts.instructions.as_ref() == [I::ReturnNull]
                {
                    // Empty-body `[ReturnNull]` untyped plain function -> single-op CallNoop!
                    ops.push(FastOp::new(
                        Op::CallNoop,
                        dst.0 as u8,
                        args.0 as u8,
                        count as u8,
                        0,
                    ));
                } else {
                    if !direct_callees.contains(&callee_idx) {
                        direct_callees.push(callee_idx);
                    }
                    ops.push(FastOp::new(
                        Op::CallFast,
                        dst.0 as u8,
                        args.0 as u8,
                        count as u8,
                        callee_idx.0 as i32,
                    ));
                }
                bc_ip.push(i as u32);
            }
            I::Return { src } => {
                ops.push(FastOp::new(Op::ReturnReg, src.0 as u8, 0, 0, 0));
                bc_ip.push(i as u32);
            }
            I::ReturnNull => {
                ops.push(FastOp::new(Op::ReturnNull, 0, 0, 0, 0));
                bc_ip.push(i as u32);
            }
            _ => {
                ops.push(FastOp::new(Op::Deopt, 0, 0, 0, 0));
                bc_ip.push(i as u32);
            }
        }
        i += 1;
    }

    // Patch all branch targets from bytecode IPs to FastOp indices.
    for (op_pos, bc_target, kind) in patch_jumps {
        let fast_target = bc_to_fast
            .get(bc_target)
            .copied()
            .filter(|&t| t != u16::MAX)
            .unwrap_or(ops.len() as u16) as usize;
        if fast_target >= 32_768 {
            return None;
        }
        match kind {
            JumpPatch::Imm => {
                ops[op_pos].imm = fast_target as i32;
            }
            JumpPatch::Bc16 => {
                ops[op_pos].b = (fast_target & 0xff) as u8;
                ops[op_pos].c = ((fast_target >> 8) & 0xff) as u8;
            }
            JumpPatch::Bc15When(when) => {
                ops[op_pos].b = (fast_target & 0xff) as u8;
                ops[op_pos].c = (((fast_target >> 8) & 0x7f) as u8) | if when { 0x80 } else { 0 };
            }
        }
    }

    let is_empty_return_null = instructions.as_ref() == [I::ReturnNull];
    let local_count = locals.min(255) as u8;

    // Post-pass 0: Peel rotated-loop entry `Jump(cond)` into an inverted guard
    // branch so `cond` is no longer a jump target, unlocking `IncLoop*`/`DecLoop*`
    // and all inner-loop fusions!
    peel_rotated_loop_entries(&mut ops, &mut bc_ip, &mut bc_to_fast);

    // Post-pass 1: Fuse tight array inner loops (`ArrAddFromArrLoop`, `ArrUnsetStepLoop`).
    fuse_array_inner_loops(&mut ops, &mut bc_ip, &mut bc_to_fast, local_count);

    // Post-pass 2: Extract unboxed `ScalarLoop` kernels and insert `EnterScalarLoop`.
    let scalar_loops = extract_scalar_loops(
        &mut ops,
        &mut bc_ip,
        &mut bc_to_fast,
        local_count,
        &f64_consts,
        &float_regs,
    );

    // Post-pass 3: Compile pure-`i64` self-recursion kernel (`fibo_r`, `Ack`).
    let int_rec = crate::fast_tier::int_rec::try_compile_int_rec(
        &ops,
        &bc_ip,
        function.registers as u8,
        local_count,
        facts.parameters as u8,
        facts.shape == FunctionShape::Plain,
        facts.takes_values_only(),
        facts.typed_parameters(),
        facts.return_mask.is_some(),
    );

    Some((
        FastFunction {
            ops: ops.into_boxed_slice(),
            bc_ip: bc_ip.into_boxed_slice(),
            bc_to_fast: bc_to_fast.into_boxed_slice(),
            f64_consts: f64_consts.into_boxed_slice(),
            scalar_loops: scalar_loops.into_boxed_slice(),
            int_rec,
            type_deopts: AtomicU8::new(0),
            reg_count: function.registers as u8,
            local_count,
            param_count: facts.parameters as u8,
            typed_params: facts.typed_parameters(),
            param_masks: facts.parameter_masks.clone(),
            return_mask: facts.return_mask,
            takes_values_only: facts.takes_values_only(),
            is_plain: facts.shape == FunctionShape::Plain,
            is_empty_return_null,
        },
        direct_callees,
    ))
}

/// Peel `Jump(cond)` -> inverted guard at entry of bottom-tested rotated loops,
/// then fuse `IncLocalFast/DecLocalFast + Branch*` -> `IncLoop*`/`DecLoop*`.
fn peel_rotated_loop_entries(ops: &mut Vec<FastOp>, bc_ip: &mut Vec<u32>, bc_to_fast: &mut [u16]) {
    let n = ops.len();
    // Step 1: Invert `Jump(cond)` where `ops[cond]` branches back to `j + 1`.
    for j in 0..n {
        if ops[j].op != Op::Jump {
            continue;
        }
        let cond = ops[j].imm as usize;
        if cond <= j + 1 || cond >= n || cond + 1 >= 32_768 {
            continue;
        }
        let exit_target = cond + 1;
        let cond_op = ops[cond];
        let inverted = match cond_op.op {
            Op::BranchCmp if cond_op.imm as usize == j + 1 => Some(FastOp::new(
                Op::BranchCmp,
                cond_op.a ^ 0x10,
                cond_op.b,
                cond_op.c,
                exit_target as i32,
            )),
            Op::BranchLtImm
            | Op::BranchLeImm
            | Op::BranchGtImm
            | Op::BranchGeImm
            | Op::BranchEqImm
            | Op::BranchNeImm
            | Op::BranchIdentImm
            | Op::BranchNotIdentImm => {
                let target = (cond_op.b as usize) | (((cond_op.c & 0x7f) as usize) << 8);
                if target == j + 1 {
                    let inv_when = (cond_op.c & 0x80) ^ 0x80;
                    Some(FastOp::new(
                        cond_op.op,
                        cond_op.a,
                        (exit_target & 0xff) as u8,
                        (((exit_target >> 8) & 0x7f) as u8) | inv_when,
                        cond_op.imm,
                    ))
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(guard) = inverted {
            ops[j] = guard;
            bc_ip[j] = bc_ip[cond];
        }
    }

    // Step 2: Recompute jump targets and fuse `IncLocalFast/DecLocalFast + Branch*`.
    let mut is_target = vec![false; n + 1];
    for &op in ops.iter() {
        if let Some(t) = crate::fast_tier::scalar_loop::fast_op_branch_target(op)
            && t < is_target.len()
        {
            is_target[t] = true;
        }
    }

    let mut fused_at: Vec<usize> = Vec::new();
    let mut i = 0usize;
    while i + 1 < n {
        if !is_target[i + 1] && matches!(ops[i].op, Op::IncLocalFast | Op::DecLocalFast) {
            let is_inc = ops[i].op == Op::IncLocalFast;
            let local = ops[i].a;
            let next = ops[i + 1];
            let fused = match next.op {
                Op::BranchLtImm | Op::BranchLeImm | Op::BranchGeImm | Op::BranchGtImm
                    if next.a == local && (next.c & 0x80) != 0 =>
                {
                    let target = (next.b as usize) | (((next.c & 0x7f) as usize) << 8);
                    let fop = match (is_inc, next.op) {
                        (true, Op::BranchLtImm) => Some(Op::IncLoopLtImm),
                        (true, Op::BranchLeImm) => Some(Op::IncLoopLeImm),
                        (false, Op::BranchGeImm) => Some(Op::DecLoopGeImm),
                        (false, Op::BranchGtImm) => Some(Op::DecLoopGtImm),
                        _ => None,
                    };
                    fop.map(|op| {
                        FastOp::new(
                            op,
                            local,
                            (target & 0xff) as u8,
                            ((target >> 8) & 0xff) as u8,
                            next.imm,
                        )
                    })
                }
                Op::BranchCmp if (next.a & 0x10) != 0 && next.b == local => {
                    let cmp = next.a & 0x0f;
                    let fop = match (is_inc, cmp) {
                        (true, CMP_LT) => Some(Op::IncLoopLtReg),
                        (true, CMP_LE) => Some(Op::IncLoopLeReg),
                        (false, CMP_GE) => Some(Op::DecLoopGeReg),
                        (false, CMP_GT) => Some(Op::DecLoopGtReg),
                        _ => None,
                    };
                    fop.map(|op| FastOp::new(op, local, next.c, 0, next.imm))
                }
                _ => None,
            };
            if let Some(fop) = fused {
                ops[i] = fop;
                fused_at.push(i + 1);
                i += 2;
                continue;
            }
        }
        i += 1;
    }

    if fused_at.is_empty() {
        return;
    }

    // Compact away the fused second instructions and remap all targets + `bc_to_fast`.
    let mut skip = vec![false; n];
    for idx in fused_at {
        skip[idx] = true;
    }
    let mut old_to_new = vec![0usize; n + 1];
    let mut new_ops = Vec::with_capacity(n);
    let mut new_bc_ip = Vec::with_capacity(n);
    for idx in 0..n {
        old_to_new[idx] = new_ops.len();
        if !skip[idx] {
            new_ops.push(ops[idx]);
            new_bc_ip.push(bc_ip[idx]);
        }
    }
    old_to_new[n] = new_ops.len();

    for op in &mut new_ops {
        remap_fast_op_target(op, &old_to_new);
    }
    for slot in bc_to_fast.iter_mut() {
        if *slot != u16::MAX && (*slot as usize) < old_to_new.len() {
            if skip[*slot as usize] {
                *slot = u16::MAX;
            } else {
                *slot = old_to_new[*slot as usize] as u16;
            }
        }
    }
    *ops = new_ops;
    *bc_ip = new_bc_ip;
}

fn fuse_array_inner_loops(
    ops: &mut Vec<FastOp>,
    bc_ip: &mut Vec<u32>,
    bc_to_fast: &mut [u16],
    local_count: u8,
) {
    let n = ops.len();
    let mut is_target = vec![false; n + 1];
    for &op in ops.iter() {
        if let Some(t) = crate::fast_tier::scalar_loop::fast_op_branch_target(op)
            && t < is_target.len()
        {
            is_target[t] = true;
        }
    }

    let mut skip = vec![false; n];
    let mut changed = false;
    let mut i = 0usize;
    while i < n {
        // Pattern 1a: `ArrAddFromArr(Y, X, j) + [Clear* ] + DecLoopGeImm(j, target=i, lim)`
        // Pattern 1b: `[Move(t_idx, j)] + ArrGetInt(t_val, X, t_idx|j) + ArrAddAssignInt(Y, j, t_val) + [Clear* ] + DecLoopGeImm(j, target=i, lim)`
        if let Some((y_reg, x_reg, j_reg, end_idx, lim)) =
            match_arr_add_loop(ops, &is_target, i, local_count)
        {
            ops[i] = FastOp::new(Op::ArrAddFromArrLoop, y_reg, x_reg, j_reg, lim);
            skip[(i + 1)..=end_idx].fill(true);
            changed = true;
            i = end_idx + 1;
            continue;
        }

        // Pattern 2: `ArrUnsetInt(flags, k) + [Clear* ] + AddAssign(k, step) + [Clear* ] + BranchLeImm(k, when=true, target=i, lim)`
        if let Some((arr_reg, k_reg, step_reg, end_idx, lim)) =
            match_arr_unset_step_loop(ops, &is_target, i, local_count)
        {
            ops[i] = FastOp::new(Op::ArrUnsetStepLoop, arr_reg, k_reg, step_reg, lim);
            skip[(i + 1)..=end_idx].fill(true);
            changed = true;
            i = end_idx + 1;
            continue;
        }
        i += 1;
    }

    if !changed {
        return;
    }

    let mut old_to_new = vec![0usize; n + 1];
    let mut new_ops = Vec::with_capacity(n);
    let mut new_bc_ip = Vec::with_capacity(n);
    for idx in 0..n {
        old_to_new[idx] = new_ops.len();
        if !skip[idx] {
            new_ops.push(ops[idx]);
            new_bc_ip.push(bc_ip[idx]);
        }
    }
    old_to_new[n] = new_ops.len();
    for op in &mut new_ops {
        remap_fast_op_target(op, &old_to_new);
    }
    for slot in bc_to_fast.iter_mut() {
        if *slot != u16::MAX && (*slot as usize) < old_to_new.len() {
            if skip[*slot as usize] {
                *slot = u16::MAX;
            } else {
                *slot = old_to_new[*slot as usize] as u16;
            }
        }
    }
    *ops = new_ops;
    *bc_ip = new_bc_ip;
}

fn match_arr_add_loop(
    ops: &[FastOp],
    is_target: &[bool],
    start: usize,
    local_count: u8,
) -> Option<(u8, u8, u8, usize, i32)> {
    let n = ops.len();
    let (y_reg, x_reg, j_reg, mut p) = if ops[start].op == Op::ArrAddFromArr {
        (ops[start].a, ops[start].b, ops[start].c, start + 1)
    } else {
        // Optional `Move(t_idx, j)`
        let mut q = start;
        let (idx_alias, orig_j) = if q < n && ops[q].op == Op::Move && ops[q].a >= local_count {
            let pair = (ops[q].a, ops[q].b);
            q += 1;
            if q >= n || is_target[q] {
                return None;
            }
            (Some(pair.0), Some(pair.1))
        } else {
            (None, None)
        };
        if q + 1 >= n || ops[q].op != Op::ArrGetInt || ops[q].imm != 0 || ops[q].a < local_count {
            return None;
        }
        let t_val = ops[q].a;
        let x_reg = ops[q].b;
        let get_idx = ops[q].c;
        let j_reg = if Some(get_idx) == idx_alias {
            orig_j?
        } else if idx_alias.is_none() {
            get_idx
        } else {
            return None;
        };
        q += 1;
        if is_target[q]
            || ops[q].op != Op::ArrAddAssignInt
            || ops[q].b != j_reg
            || ops[q].c != t_val
        {
            return None;
        }
        (ops[q].a, x_reg, j_reg, q + 1)
    };

    // Skip any `Clear(temp)` ops before the latch.
    while p < n && !is_target[p] && ops[p].op == Op::Clear && ops[p].a >= local_count {
        p += 1;
    }
    if p < n
        && !is_target[p]
        && ops[p].op == Op::DecLoopGeImm
        && ops[p].a == j_reg
        && ((ops[p].b as usize) | ((ops[p].c as usize) << 8)) == start
    {
        Some((y_reg, x_reg, j_reg, p, ops[p].imm))
    } else {
        None
    }
}

fn match_arr_unset_step_loop(
    ops: &[FastOp],
    is_target: &[bool],
    start: usize,
    local_count: u8,
) -> Option<(u8, u8, u8, usize, i32)> {
    let n = ops.len();
    if ops[start].op != Op::ArrUnsetInt {
        return None;
    }
    let arr_reg = ops[start].a;
    let k_reg = ops[start].b;
    let mut p = start + 1;
    while p < n && !is_target[p] && ops[p].op == Op::Clear && ops[p].a >= local_count {
        p += 1;
    }
    if p >= n || is_target[p] || ops[p].op != Op::AddAssign || ops[p].a != k_reg {
        return None;
    }
    let step_reg = ops[p].b;
    p += 1;
    while p < n && !is_target[p] && ops[p].op == Op::Clear && ops[p].a >= local_count {
        p += 1;
    }
    if p < n
        && !is_target[p]
        && ops[p].op == Op::BranchLeImm
        && ops[p].a == k_reg
        && (ops[p].c & 0x80) != 0
        && ((ops[p].b as usize) | (((ops[p].c & 0x7f) as usize) << 8)) == start
    {
        Some((arr_reg, k_reg, step_reg, p, ops[p].imm))
    } else {
        None
    }
}

fn extract_scalar_loops(
    ops: &mut Vec<FastOp>,
    bc_ip: &mut Vec<u32>,
    bc_to_fast: &mut [u16],
    local_count: u8,
    f64_consts: &[f64],
    float_hints: &[bool],
) -> Vec<crate::fast_tier::scalar_loop::ScalarLoop> {
    use crate::fast_tier::scalar_loop::{fast_op_branch_target, try_compile_scalar_loop};

    // 1. Find disjoint innermost candidate loops `(start, end)` that compile cleanly.
    let mut candidates: Vec<(usize, usize)> = Vec::new();
    for (end, &op) in ops.iter().enumerate() {
        let Some(start) = fast_op_branch_target(op) else {
            continue;
        };
        if start > end {
            continue;
        }
        // Only disjoint innermost loops (skip if overlaps an existing candidate).
        if candidates.iter().any(|&(s, e)| !(end < s || start > e)) {
            continue;
        }
        if try_compile_scalar_loop(
            ops,
            start,
            start,
            end,
            start,
            local_count,
            f64_consts,
            float_hints,
        )
        .is_some()
        {
            candidates.push((start, end));
        }
    }
    if candidates.is_empty() || ops.len() + candidates.len() >= 32_767 {
        return Vec::new();
    }
    candidates.sort_by_key(|&(s, _)| s);

    // 2. Insert `Op::EnterScalarLoop(id)` before each `start`, keeping a complete
    //    `old_to_new` map to patch every jump target and `bc_to_fast` cleanly.
    let old_len = ops.len();
    let mut old_to_new = vec![0usize; old_len + 1];
    let mut new_ops = Vec::with_capacity(old_len + candidates.len());
    let mut new_bc_ip = Vec::with_capacity(old_len + candidates.len());
    let mut new_spans: Vec<(usize, usize)> = Vec::with_capacity(candidates.len());

    let mut cand_i = 0usize;
    for old_i in 0..old_len {
        if cand_i < candidates.len() && candidates[cand_i].0 == old_i {
            let header_pos = new_ops.len();
            old_to_new[old_i] = header_pos;
            new_ops.push(FastOp::new(Op::EnterScalarLoop, 0, 0, 0, cand_i as i32));
            new_bc_ip.push(bc_ip[old_i]);
            new_spans.push((header_pos, 0));
            cand_i += 1;
        } else {
            old_to_new[old_i] = new_ops.len();
        }
        let pushed_pos = new_ops.len();
        new_ops.push(ops[old_i]);
        new_bc_ip.push(bc_ip[old_i]);
        if cand_i > 0 && candidates[cand_i - 1].1 == old_i {
            new_spans[cand_i - 1].1 = pushed_pos;
        }
    }
    old_to_new[old_len] = new_ops.len();

    // 3. Remap every jump target in `new_ops` and every entry in `bc_to_fast`.
    for op in &mut new_ops {
        remap_fast_op_target(op, &old_to_new);
    }
    for slot in bc_to_fast.iter_mut() {
        if *slot != u16::MAX && (*slot as usize) < old_to_new.len() {
            *slot = old_to_new[*slot as usize] as u16;
        }
    }

    // 4. Compile each `ScalarLoop` directly from the FINAL `new_ops` slice!
    let mut loops = Vec::with_capacity(new_spans.len());
    for &(header_pos, end_pos) in &new_spans {
        let Some(compiled) = try_compile_scalar_loop(
            &new_ops,
            header_pos,
            header_pos + 1,
            end_pos,
            header_pos,
            local_count,
            f64_consts,
            float_hints,
        ) else {
            return Vec::new();
        };
        loops.push(compiled);
    }

    *ops = new_ops;
    *bc_ip = new_bc_ip;
    loops
}

fn remap_fast_op_target(op: &mut FastOp, old_to_new: &[usize]) {
    match op.op {
        Op::Jump
        | Op::JumpIf
        | Op::JumpIfNot
        | Op::JumpIfNull
        | Op::JumpIfNotNull
        | Op::BranchCmp
        | Op::IncLoopLtReg
        | Op::IncLoopLeReg
        | Op::DecLoopGeReg
        | Op::DecLoopGtReg => {
            if let Some(&mapped) = old_to_new.get(op.imm as usize) {
                op.imm = mapped as i32;
            }
        }
        Op::IncLoopLtImm | Op::IncLoopLeImm | Op::DecLoopGeImm | Op::DecLoopGtImm => {
            let old_t = (op.b as usize) | ((op.c as usize) << 8);
            if let Some(&mapped) = old_to_new.get(old_t) {
                op.b = (mapped & 0xff) as u8;
                op.c = ((mapped >> 8) & 0xff) as u8;
            }
        }
        Op::BranchLtImm
        | Op::BranchLeImm
        | Op::BranchGtImm
        | Op::BranchGeImm
        | Op::BranchEqImm
        | Op::BranchNeImm
        | Op::BranchIdentImm
        | Op::BranchNotIdentImm => {
            let when = op.c & 0x80;
            let old_t = (op.b as usize) | (((op.c & 0x7f) as usize) << 8);
            if let Some(&mapped) = old_to_new.get(old_t) {
                op.b = (mapped & 0xff) as u8;
                op.c = (((mapped >> 8) & 0x7f) as u8) | when;
            }
        }
        _ => {}
    }
}

/// Propagate float-type hints across two linear passes so float arithmetic
/// (`mandel`, `mandel2`, `heapsort`) uses float-first `AddF64`/`SubF64`/`MulF64`.
/// Soundness never depends on this hint: every float-first op still handles
/// ints and mixed operands via its out-of-line slow helper!
fn infer_float_hints(function: &Function, unit: &CompiledUnit) -> Vec<bool> {
    let mut is_float = vec![false; function.registers as usize];
    for _ in 0..3 {
        let mut changed = false;
        for &inst in function.instructions.iter() {
            let mark = |is_float: &mut [bool], dst: Reg, val: bool, changed: &mut bool| {
                if val
                    && let Some(slot) = is_float.get_mut(dst.index())
                    && !*slot
                {
                    *slot = true;
                    *changed = true;
                }
            };
            match inst {
                I::LoadConst { dst, constant }
                | I::AssignConst {
                    local: dst,
                    constant,
                } => {
                    if matches!(
                        unit.constants.get(constant.index()),
                        Some(Constant::Float(_))
                    ) {
                        mark(&mut is_float, dst, true, &mut changed);
                    }
                }
                I::Cast {
                    to: CastKind::Float,
                    dst,
                    ..
                } => mark(&mut is_float, dst, true, &mut changed),
                I::Move { dst, src }
                | I::Assign { local: dst, src }
                | I::AssignTake { local: dst, src }
                | I::Unary {
                    op: UnaryOp::Negate,
                    dst,
                    src,
                } => {
                    let f = is_float.get(src.index()).copied().unwrap_or(false);
                    mark(&mut is_float, dst, f, &mut changed);
                }
                I::Binary { op, dst, lhs, rhs } => {
                    let lf = is_float.get(lhs.index()).copied().unwrap_or(false);
                    let rf = is_float.get(rhs.index()).copied().unwrap_or(false);
                    if op == BinaryOp::Divide
                        || (matches!(op, BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply)
                            && (lf || rf))
                    {
                        mark(&mut is_float, dst, true, &mut changed);
                    }
                }
                I::BinaryInt { op, dst, lhs, .. } => {
                    let lf = is_float.get(lhs.index()).copied().unwrap_or(false);
                    if op == BinaryOp::Divide
                        || (matches!(op, BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply)
                            && lf)
                    {
                        mark(&mut is_float, dst, true, &mut changed);
                    }
                }
                _ => {}
            }
        }
        if !changed {
            break;
        }
    }
    is_float
}

#[derive(Clone, Copy)]
enum JumpPatch {
    Imm,
    Bc16,
    Bc15When(bool),
}

fn try_lower_producer(
    inst: I,
    expected_dst: Reg,
    target_local: Reg,
    function: &Function,
    unit: &CompiledUnit,
    float_regs: &[bool],
    f64_consts: &mut Vec<f64>,
) -> Option<FastOp> {
    let dst_u8 = target_local.0 as u8;
    match inst {
        I::LoadInt { dst, value } if dst == expected_dst => {
            Some(FastOp::new(Op::LoadInt, dst_u8, 0, 0, value))
        }
        I::LoadBool { dst, value } if dst == expected_dst => {
            Some(FastOp::new(Op::LoadBool, dst_u8, u8::from(value), 0, 0))
        }
        I::LoadNull { dst } if dst == expected_dst => {
            Some(FastOp::new(Op::LoadNull, dst_u8, 0, 0, 0))
        }
        I::LoadConst { dst, constant } if dst == expected_dst => {
            Some(lower_load_const(dst_u8, constant.0, unit, f64_consts))
        }
        I::Binary { op, dst, lhs, rhs } if dst == expected_dst => {
            let flt = float_regs[lhs.index()] || float_regs[rhs.index()];
            lower_binary(op, dst_u8, lhs.0 as u8, rhs.0 as u8, flt)
        }
        I::BinaryInt { op, dst, lhs, rhs } if dst == expected_dst => {
            let flt = float_regs[lhs.index()];
            lower_binary_int(op, dst_u8, lhs.0 as u8, rhs, flt)
        }
        I::Unary {
            op: UnaryOp::Negate,
            dst,
            src,
        } if dst == expected_dst => Some(FastOp::new(Op::NegNum, dst_u8, src.0 as u8, 0, 0)),
        I::Cast { to, dst, src } if dst == expected_dst => match to {
            CastKind::Int => Some(FastOp::new(Op::CastInt, dst_u8, src.0 as u8, 0, 0)),
            CastKind::Float => Some(FastOp::new(Op::CastFloat, dst_u8, src.0 as u8, 0, 0)),
            CastKind::Bool => Some(FastOp::new(Op::CastBool, dst_u8, src.0 as u8, 0, 0)),
            _ => None,
        },
        I::FetchIndex {
            dst,
            base,
            index,
            mode,
            ..
        } if dst == expected_dst => {
            let quiet = matches!(mode, FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty);
            Some(FastOp::new(
                Op::ArrGetInt,
                dst_u8,
                base.0 as u8,
                index.0 as u8,
                i32::from(quiet),
            ))
        }
        I::Fetch { dst, path, mode } if dst == expected_dst => {
            lower_fetch_2d(function, dst_u8, path, mode)
        }
        _ => None,
    }
}

fn lower_load_const(
    dst: u8,
    const_id: u32,
    unit: &CompiledUnit,
    f64_consts: &mut Vec<f64>,
) -> FastOp {
    match unit.constants.get(const_id as usize) {
        Some(Constant::Null) => FastOp::new(Op::LoadNull, dst, 0, 0, 0),
        Some(Constant::Bool(b)) => FastOp::new(Op::LoadBool, dst, u8::from(*b), 0, 0),
        Some(Constant::Int(v)) => {
            if let Ok(i) = i32::try_from(*v) {
                FastOp::new(Op::LoadInt, dst, 0, 0, i)
            } else {
                FastOp::new(Op::LoadConstRef, dst, 0, 0, const_id as i32)
            }
        }
        Some(Constant::Float(f)) => {
            let bits = f.to_bits();
            let idx = if let Some(pos) = f64_consts.iter().position(|x| x.to_bits() == bits) {
                pos
            } else {
                let pos = f64_consts.len();
                f64_consts.push(*f);
                pos
            };
            FastOp::new(Op::LoadF64, dst, 0, 0, idx as i32)
        }
        Some(Constant::Array(arr)) if arr.is_empty() => FastOp::new(Op::NewArray, dst, 0, 0, 0),
        Some(_) => FastOp::new(Op::LoadConstRef, dst, 0, 0, const_id as i32),
        None => FastOp::new(Op::Deopt, 0, 0, 0, 0),
    }
}

fn lower_fetch_2d(
    function: &Function,
    dst: u8,
    path: phera_bytecode::PathId,
    mode: FetchMode,
) -> Option<FastOp> {
    if mode == FetchMode::Destructure {
        return None;
    }
    let def = function.paths.get(path.index())?;
    let PathBase::Register(base) = def.base else {
        return None;
    };
    let [Step::Index(i1), Step::Index(i2)] = def.steps.as_slice() else {
        return None;
    };
    let quiet = matches!(mode, FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty);
    let imm = i32::from(i2.0 as u8) | (i32::from(quiet) << 8);
    Some(FastOp::new(
        Op::Arr2DGetInt,
        dst,
        base.0 as u8,
        i1.0 as u8,
        imm,
    ))
}

fn lower_binary(op: BinaryOp, dst: u8, lhs: u8, rhs: u8, float_hint: bool) -> Option<FastOp> {
    let fast_op = match op {
        BinaryOp::Add => {
            if float_hint {
                Op::AddF64
            } else {
                Op::AddNum
            }
        }
        BinaryOp::Subtract => {
            if float_hint {
                Op::SubF64
            } else {
                Op::SubNum
            }
        }
        BinaryOp::Multiply => {
            if float_hint {
                Op::MulF64
            } else {
                Op::MulNum
            }
        }
        BinaryOp::Divide => {
            if float_hint {
                Op::DivF64
            } else {
                Op::DivNum
            }
        }
        BinaryOp::Modulo => Op::ModInt,
        BinaryOp::BitwiseAnd => Op::BitAnd,
        BinaryOp::BitwiseOr => Op::BitOr,
        BinaryOp::BitwiseXor => Op::BitXor,
        BinaryOp::ShiftLeft => Op::Shl,
        BinaryOp::ShiftRight => Op::Shr,
        _ => {
            let cmp = cmp_code(op)?;
            return Some(FastOp::new(Op::CmpNum, dst, lhs, rhs, i32::from(cmp)));
        }
    };
    Some(FastOp::new(fast_op, dst, lhs, rhs, 0))
}

fn lower_binary_int(op: BinaryOp, dst: u8, lhs: u8, rhs: i32, float_hint: bool) -> Option<FastOp> {
    let fast_op = match op {
        BinaryOp::Add => {
            if float_hint {
                Op::AddF64Imm
            } else {
                Op::AddImm
            }
        }
        BinaryOp::Subtract => {
            if float_hint {
                Op::SubF64Imm
            } else {
                Op::SubImm
            }
        }
        BinaryOp::Multiply => {
            if float_hint {
                Op::MulF64Imm
            } else {
                Op::MulImm
            }
        }
        BinaryOp::Modulo => Op::ModImm,
        BinaryOp::BitwiseAnd => Op::BitAndImm,
        BinaryOp::BitwiseOr => Op::BitOrImm,
        BinaryOp::BitwiseXor => Op::BitXorImm,
        BinaryOp::ShiftLeft => Op::ShlImm,
        BinaryOp::ShiftRight => Op::ShrImm,
        _ => return None,
    };
    Some(FastOp::new(fast_op, dst, lhs, 0, rhs))
}

fn cmp_code(op: BinaryOp) -> Option<u8> {
    match op {
        BinaryOp::Equal => Some(CMP_EQ),
        BinaryOp::NotEqual => Some(CMP_NE),
        BinaryOp::Identical => Some(CMP_IDENT),
        BinaryOp::NotIdentical => Some(CMP_NOT_IDENT),
        BinaryOp::Less => Some(CMP_LT),
        BinaryOp::LessEqual => Some(CMP_LE),
        BinaryOp::Greater => Some(CMP_GT),
        BinaryOp::GreaterEqual => Some(CMP_GE),
        _ => None,
    }
}

fn branch_imm_op(op: BinaryOp) -> Option<Op> {
    match op {
        BinaryOp::Less => Some(Op::BranchLtImm),
        BinaryOp::LessEqual => Some(Op::BranchLeImm),
        BinaryOp::Greater => Some(Op::BranchGtImm),
        BinaryOp::GreaterEqual => Some(Op::BranchGeImm),
        BinaryOp::Equal => Some(Op::BranchEqImm),
        BinaryOp::NotEqual => Some(Op::BranchNeImm),
        BinaryOp::Identical => Some(Op::BranchIdentImm),
        BinaryOp::NotIdentical => Some(Op::BranchNotIdentImm),
        _ => None,
    }
}

fn jump_target(inst: I) -> Option<Target> {
    match inst {
        I::Jump { target }
        | I::JumpIf { target, .. }
        | I::JumpIfNot { target, .. }
        | I::JumpIfNull { target, .. }
        | I::JumpIfNotNull { target, .. }
        | I::BinaryBranch { target, .. }
        | I::BinaryBranchIf { target, .. }
        | I::BinaryIntBranch { target, .. }
        | I::BinaryIntBranchIf { target, .. }
        | I::InstanceOfBranch { target, .. }
        | I::InstanceOfBranchIf { target, .. }
        | I::FinallyCall { target, .. } => Some(target),
        I::IterStart { end, .. }
        | I::IterStartReference { end, .. }
        | I::IterNext { end, .. }
        | I::IterNextKeyed { end, .. } => Some(end),
        I::ReceiveDefault { passed, .. } => Some(passed),
        I::GuardBuiltin { call, .. } => Some(call),
        _ => None,
    }
}

fn temp_dead_after(
    instructions: &[I],
    is_target: &[bool],
    after_idx: usize,
    reg: Reg,
    function: &Function,
) -> bool {
    let mut k = after_idx + 1;
    while k < instructions.len() {
        if is_target[k] {
            // Stop conservatively at merge points unless never read anywhere.
            return !reg_read_anywhere(instructions, function, reg);
        }
        let inst = instructions[k];
        if inst_reads_reg(function, inst, reg) {
            return false;
        }
        if inst_overwrites_reg(inst, reg) {
            return true;
        }
        if matches!(
            inst,
            I::Return { .. } | I::ReturnNull | I::Throw { .. } | I::Exit { .. }
        ) {
            return true;
        }
        if jump_target(inst).is_some() {
            return !reg_read_anywhere(instructions, function, reg);
        }
        k += 1;
    }
    true
}

fn reg_read_anywhere(instructions: &[I], function: &Function, reg: Reg) -> bool {
    instructions
        .iter()
        .any(|&inst| inst_reads_reg(function, inst, reg))
}

fn inst_reads_reg(function: &Function, inst: I, reg: Reg) -> bool {
    let mut found = false;
    let mut check = |r: Reg| {
        if r == reg {
            found = true;
        }
    };
    visit_reads(function, inst, &mut check);
    found
}

fn inst_overwrites_reg(inst: I, reg: Reg) -> bool {
    match inst {
        I::LoadInt { dst, .. }
        | I::LoadBool { dst, .. }
        | I::LoadNull { dst }
        | I::LoadConst { dst, .. }
        | I::AssignConst { local: dst, .. }
        | I::NewArray { dst, .. }
        | I::Move { dst, .. }
        | I::Assign { local: dst, .. }
        | I::AssignTake { local: dst, .. }
        | I::Clear { reg: dst }
        | I::Binary { dst, .. }
        | I::BinaryInt { dst, .. }
        | I::Unary { dst, .. }
        | I::Cast { dst, .. }
        | I::BuiltinUnary { dst, .. }
        | I::IncDecLocal { dst, .. }
        | I::FetchIndex { dst, .. }
        | I::Fetch { dst, .. }
        | I::CallDirect { dst, .. }
        | I::CallNativeDirect { dst, .. }
        | I::CallMethod { dst, .. }
        | I::Call { dst, .. } => dst == reg,
        _ => false,
    }
}

fn count_reads(function: &Function, inst: I, reads: &mut [u32]) {
    visit_reads(function, inst, &mut |r: Reg| {
        if let Some(slot) = reads.get_mut(r.index()) {
            *slot += 1;
        }
    });
}

fn visit_reads(function: &Function, inst: I, read: &mut impl FnMut(Reg)) {
    match inst {
        I::Move { src, .. }
        | I::Assign { src, .. }
        | I::AssignTake { src, .. }
        | I::Unary { src, .. }
        | I::Cast { src, .. }
        | I::Return { src }
        | I::GeneratorReturn { src }
        | I::Throw { source: src }
        | I::CheckReturn { source: src }
        | I::ReturnChecked { source: src }
        | I::YieldFrom { src, .. }
        | I::SendValue { src, .. }
        | I::SendTake { src, .. }
        | I::SendUserCallback { src, .. }
        | I::Echo { src }
        | I::Exit { src }
        | I::Eval { source: src, .. }
        | I::Include { path: src, .. }
        | I::BuiltinUnary { src, .. }
        | I::NamespacedBuiltin { src, .. }
        | I::InLiteralArray { needle: src, .. }
        | I::Capture { src, .. }
        | I::InitDynamicCall { callee: src }
        | I::InitUserCallback { callback: src, .. }
        | I::InstanceOf { src, .. }
        | I::InstanceOfBranch { src, .. }
        | I::InstanceOfBranchIf { src, .. }
        | I::FetchClassConstant { class: src, .. }
        | I::IterStart { src, .. }
        | I::BindDynamicGlobal { name: src } => read(src),
        I::Clone { src, with, .. } => {
            read(src);
            if let Some(w) = with {
                read(w);
            }
        }
        I::Yield { key, value, .. } => {
            if let Some(k) = key {
                read(k);
            }
            if let Some(v) = value {
                read(v);
            }
        }
        I::SendLocal { local }
        | I::IssetLocal { local, .. }
        | I::IncDecLocal { local, .. }
        | I::CheckType { local, .. } => read(local),
        I::AssignOp { local, src, .. } | I::AppendLocal { local, src } => {
            read(local);
            read(src);
        }
        I::Binary { lhs, rhs, .. }
        | I::BinaryBranch { lhs, rhs, .. }
        | I::BinaryBranchIf { lhs, rhs, .. } => {
            read(lhs);
            read(rhs);
        }
        I::BinaryInt { lhs, .. }
        | I::BinaryIntBranch { lhs, .. }
        | I::BinaryIntBranchIf { lhs, .. } => read(lhs),
        I::JumpIf { condition, .. } | I::JumpIfNot { condition, .. } => read(condition),
        I::JumpIfNull { value, .. } | I::JumpIfNotNull { value, .. } => read(value),
        I::ArrayPush { array, src }
        | I::ArrayPushReference { array, src }
        | I::ArraySpread { array, src } => {
            read(array);
            read(src);
        }
        I::ArraySet { array, key, src } | I::ArraySetReference { array, key, src } => {
            read(array);
            read(key);
            read(src);
        }
        I::ArrayKeyExists { key, array, .. } => {
            read(key);
            read(array);
        }
        I::FetchIndex { base, index, .. } => {
            read(base);
            read(index);
        }
        I::Fetch { path, .. }
        | I::MakeReference { path, .. }
        | I::IncDecPath { path, .. }
        | I::UnsetPath { path }
        | I::IssetPath { path, .. }
        | I::SendPath { path }
        | I::IterStartReference { path, .. } => count_path_reads(function, path, read),
        I::AssignPath { path, src }
        | I::AssignPathFromLocal { path, src, .. }
        | I::AssignOpPath { path, src, .. }
        | I::BindReference { path, src } => {
            count_path_reads(function, path, read);
            read(src);
        }
        I::CallDirect { args, count, .. } | I::CallNativeDirect { args, count, .. } => {
            for offset in 0..count {
                read(Reg(args.0 + offset));
            }
        }
        I::InitMethodCall { receiver, .. } => read(receiver),
        I::CallMethod { site, .. } => {
            if let Some(site_def) = function.method_sites.get(site.index()) {
                read(site_def.receiver);
                for arg in &function.site_arguments[site_def.argument_range()] {
                    read(arg.register());
                }
            }
        }
        I::StaticCall { class, method, .. } => {
            read(class);
            read(method);
        }
        I::FinallyReturn { state } | I::FinallyDiscard { state } => read(state),
        _ => {}
    }
}

fn count_path_reads(function: &Function, path: phera_bytecode::PathId, read: &mut impl FnMut(Reg)) {
    let Some(def) = function.paths.get(path.index()) else {
        return;
    };
    match def.base {
        PathBase::Register(r) | PathBase::Named(r) => read(r),
        PathBase::StaticProperty { class, name, .. } => {
            read(class);
            read(name);
        }
        PathBase::This { .. }
        | PathBase::NamedStaticProperty { .. }
        | PathBase::GlobalNamed { .. }
        | PathBase::Globals => {}
    }
    for step in &def.steps {
        match *step {
            Step::Index(r) | Step::DynamicProperty(r) => read(r),
            Step::Append | Step::Property(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_table::CodeTable;

    fn lower_fn(php: &str, fn_name: &str) -> Arc<FastFunction> {
        let unit = Arc::new(phera_compiler::compile(php, "t.php").expect("compile"));
        let mut code = CodeTable::default();
        let first = code.load(&unit);
        let idx = unit
            .functions
            .iter()
            .position(|f| f.name == fn_name)
            .expect("function present");
        let f_idx = code.declared(first, idx);
        let (fast, _) = lower_function(&code, f_idx, true).expect("lowered");
        fast
    }

    #[test]
    fn specializations_fire_on_standard_loop_and_recursion_shapes() {
        let simple = lower_fn(
            "<?php function simple() { $a = 0; for ($i = 0; $i < 1000; $i++) $a++; return $a; }",
            "simple",
        );
        assert!(
            !simple.scalar_loops.is_empty(),
            "simple should extract ScalarLoop; ops={:?}",
            simple.ops
        );

        let fibo = lower_fn(
            "<?php function fibo_r($n) { return ($n < 2) ? 1 : fibo_r($n - 2) + fibo_r($n - 1); }",
            "fibo_r",
        );
        assert!(
            fibo.int_rec.is_some(),
            "fibo_r should compile IntRecFunc; ops={:?}",
            fibo.ops
        );

        let ack = lower_fn(
            "<?php function Ack($m, $n) { if ($m == 0) return $n + 1; if ($n == 0) return Ack($m - 1, 1); return Ack($m - 1, Ack($m, $n - 1)); }",
            "Ack",
        );
        assert!(
            ack.int_rec.is_some(),
            "Ack should compile IntRecFunc; ops={:?}",
            ack.ops
        );

        let mandel = lower_fn(
            "<?php function mandel_inner($x, $y) { $cr = $y - 0.5; $ci = $x; $zi = 0.0; $zr = 0.0; $i = 0; while (1) { $i++; $temp = $zr * $zi; $zr2 = $zr * $zr; $zi2 = $zi * $zi; $zr = $zr2 - $zi2 + $cr; $zi = $temp + $temp + $ci; if ($zi2 + $zr2 > 16) return $i; if ($i > 1000) return 0; } }",
            "mandel_inner",
        );
        assert!(
            !mandel.scalar_loops.is_empty(),
            "mandel_inner should extract ScalarLoop; ops={:?}",
            mandel.ops
        );

        let mandel2 = lower_fn(
            "<?php function mandel2_inner($cr, $ci) { $zr = 0.0; $zi = 0.0; for ($i = 0; $i < 50; $i++) { $zr2 = $zr * $zr; $zi2 = $zi * $zi; if ($zr2 + $zi2 > 4.0) break; $zi = 2 * $zr * $zi + $ci; $zr = $zr2 - $zi2 + $cr; } return $i; }",
            "mandel2_inner",
        );
        assert!(
            !mandel2.scalar_loops.is_empty(),
            "mandel2_inner should extract ScalarLoop; ops={:?}",
            mandel2.ops
        );

        let ary3 = lower_fn(
            "<?php function ary3($n) { $X = []; $Y = []; for ($i = 0; $i < $n; $i++) { $X[$i] = $i; $Y[$i] = 0; } for ($k = 0; $k < 10; $k++) for ($j = $n - 1; $j >= 0; $j--) $Y[$j] += $X[$j]; return $Y[0]; }",
            "ary3",
        );
        assert!(
            ary3.ops.iter().any(|o| o.op == Op::ArrAddFromArrLoop),
            "ary3 should fuse ArrAddFromArrLoop; ops={:?}",
            ary3.ops
        );

        let sieve = lower_fn(
            "<?php function sieve($n) { $flags = []; for ($i = 2; $i <= 8192; $i++) $flags[$i] = 1; $c = 0; for ($i = 2; $i <= 8192; $i++) { if (isset($flags[$i])) { for ($k = $i + $i; $k <= 8192; $k += $i) unset($flags[$k]); $c++; } } return $c; }",
            "sieve",
        );
        assert!(
            sieve.ops.iter().any(|o| o.op == Op::ArrUnsetStepLoop),
            "sieve should fuse ArrUnsetStepLoop; ops={:?}",
            sieve.ops
        );
    }
}
