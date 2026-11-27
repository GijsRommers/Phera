//! Lazy bytecode -> Tier-2 [`FastFunction`] lowerer with general peephole fusions.

use std::sync::Arc;

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

/// Check whether a function is worth lowering into Tier 2.
///
/// Cold one-shot functions (no loops and no direct calls) stay on Tier 1 so
/// framework bootstrap / route dispatch pays zero compilation overhead.
pub(crate) fn is_eligible(function: &Function, require_hot: bool) -> bool {
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
    if !require_hot {
        return true;
    }
    // Lower functions that contain a backward branch (loop) or a direct call.
    for (ip, inst) in function.instructions.iter().enumerate() {
        if matches!(inst, I::CallDirect { .. }) {
            return true;
        }
        if let Some(target) = jump_target(*inst)
            && target.index() <= ip
        {
            return true;
        }
    }
    false
}

/// Lower `function_idx` (and transitively any `CallDirect` callees in the same
/// unit) into [`FastFunction`]s.
pub(crate) fn lower_function(
    code: &CodeTable,
    function_idx: FunctionIndex,
    require_hot: bool,
) -> Option<(Arc<FastFunction>, Vec<FunctionIndex>)> {
    let (function, _cache, facts) = code.function_parts(function_idx);
    if !is_eligible(function, require_hot) {
        return None;
    }
    let unit = code.unit(function_idx);
    let (fast, direct_callees) = lower_body(function, unit, facts)?;
    Some((Arc::new(fast), direct_callees))
}

fn lower_body(
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

    let mut ops: Vec<FastOp> = Vec::with_capacity(len);
    let mut bc_ip: Vec<u32> = Vec::with_capacity(len);
    let mut bc_to_fast: Vec<u16> = vec![u16::MAX; len];
    let mut f64_consts: Vec<f64> = Vec::new();
    let mut direct_callees: Vec<FunctionIndex> = Vec::new();
    let mut patch_jumps: Vec<(usize, usize, JumpPatch)> = Vec::new();

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

        // --- Fusion 2: Rotated loop latch `IncDecLocal(dead dst) + Binary*BranchIf` ---
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
            // Standalone dead-dst IncDecLocal -> IncLocalFast / DecLocalFast.
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

        // --- Fusion 3: Single-use temp producer + `Assign` / `AssignTake` -> write directly into local ---
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
                // Skip Nop (patch target falls through to next emitted op).
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
                    capacity as i32,
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
                if let Some(fast) = lower_binary(op, dst.0 as u8, lhs.0 as u8, rhs.0 as u8) {
                    ops.push(fast);
                } else {
                    ops.push(FastOp::new(Op::Deopt, 0, 0, 0, 0));
                }
                bc_ip.push(i as u32);
            }
            I::BinaryInt { op, dst, lhs, rhs } => {
                if let Some(fast) = lower_binary_int(op, dst.0 as u8, lhs.0 as u8, rhs) {
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
            lower_binary(op, dst_u8, lhs.0 as u8, rhs.0 as u8)
        }
        I::BinaryInt { op, dst, lhs, rhs } if dst == expected_dst => {
            lower_binary_int(op, dst_u8, lhs.0 as u8, rhs)
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
                FastOp::new(Op::LoadConstVal, dst, 0, 0, const_id as i32)
            }
        }
        Some(Constant::Float(f)) => {
            let idx = if let Some(pos) = f64_consts.iter().position(|x| x.to_bits() == f.to_bits())
            {
                pos
            } else {
                let pos = f64_consts.len();
                f64_consts.push(*f);
                pos
            };
            FastOp::new(Op::LoadF64, dst, 0, 0, idx as i32)
        }
        Some(_) => FastOp::new(Op::LoadConstVal, dst, 0, 0, const_id as i32),
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

fn lower_binary(op: BinaryOp, dst: u8, lhs: u8, rhs: u8) -> Option<FastOp> {
    let fast_op = match op {
        BinaryOp::Add => Op::AddNum,
        BinaryOp::Subtract => Op::SubNum,
        BinaryOp::Multiply => Op::MulNum,
        BinaryOp::Divide => Op::DivNum,
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

fn lower_binary_int(op: BinaryOp, dst: u8, lhs: u8, rhs: i32) -> Option<FastOp> {
    let fast_op = match op {
        BinaryOp::Add => Op::AddImm,
        BinaryOp::Subtract => Op::SubImm,
        BinaryOp::Multiply => Op::MulImm,
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
