//! Lazy bytecode -> Tier-2 [`FastFunction`] lowerer with:
//! - Static clean-loop / self-recursion eligibility check (eliminates deopt
//!   thrashing on OOP, native, and HTTP workloads)
//! - Float-type-hint propagation (emits float-first `AddF64`/`SubF64`/`MulF64`
//!   ops for float-dominated loops like `mandel`/`mandel2`/`heapsort`)
//! - General peephole fusions (`IncLoop*`, `DecLoop*`, `RetIntImm`,
//!   `ArrAddFromArr`, `CallSelf1`, `CallSelf2`, `CallNoop`, producer+`Assign`)

use std::sync::Arc;

use phera_bytecode::{
    BinaryOp, BuiltinUnary, CastKind, CompiledUnit, Constant, FetchMode, Function, IncDec,
    Instruction as I, PathBase, Reg, Step, Target, UnaryOp,
};

use crate::code_table::{CodeTable, FunctionIndex, FunctionShape};
use crate::fast_tier::{
    CMP_EQ, CMP_GE, CMP_GT, CMP_IDENT, CMP_LE, CMP_LT, CMP_NE, CMP_NOT_IDENT, FastFunction, FastOp,
    Op, box_f64,
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

    // Simple 2-pass float-hint propagation so float-dominated loops (`mandel`,
    // `mandel2`, `heapsort`) emit float-first `AddF64`/`SubF64`/`MulF64` ops!
    let float_regs = infer_float_hints(function, unit);

    let mut ops: Vec<FastOp> = Vec::with_capacity(len);
    let mut bc_ip: Vec<u32> = Vec::with_capacity(len);
    let mut bc_to_fast: Vec<u16> = vec![u16::MAX; len];
    let mut f64_consts: Vec<u64> = Vec::new();
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
            && reads[fetch_dst.index()] == 1
            && assign_dst.index() >= locals
            && reads[assign_dst.index()] == 0
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

        // --- Fusion 3: Rotated loop latch `IncDecLocal(dead dst) + Binary*BranchIf` ---
        if let I::IncDecLocal { mode, dst, local } = inst
            && dst.index() >= locals
            && reads[dst.index()] == 0
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
                if src.index() >= locals && reads[src.index()] == 1 =>
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

    Some((
        FastFunction {
            ops: ops.into_boxed_slice(),
            bc_ip: bc_ip.into_boxed_slice(),
            bc_to_fast: bc_to_fast.into_boxed_slice(),
            f64_consts: f64_consts.into_boxed_slice(),
            reg_count: function.registers as u8,
            local_count: locals.min(255) as u8,
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
    f64_consts: &mut Vec<u64>,
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
    f64_consts: &mut Vec<u64>,
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
            let bits = box_f64(*f);
            let idx = if let Some(pos) = f64_consts.iter().position(|&x| x == bits) {
                pos
            } else {
                let pos = f64_consts.len();
                f64_consts.push(bits);
                pos
            };
            FastOp::new(Op::LoadF64, dst, 0, 0, idx as i32)
        }
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

fn count_reads(function: &Function, inst: I, reads: &mut [u32]) {
    let mut read = |r: Reg| {
        if let Some(slot) = reads.get_mut(r.index()) {
            *slot += 1;
        }
    };
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
        | I::IterStartReference { path, .. } => count_path_reads(function, path, &mut read),
        I::AssignPath { path, src }
        | I::AssignPathFromLocal { path, src, .. }
        | I::AssignOpPath { path, src, .. }
        | I::BindReference { path, src } => {
            count_path_reads(function, path, &mut read);
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
