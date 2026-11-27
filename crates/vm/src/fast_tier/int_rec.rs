//! Unboxed raw-`i64` self-recursion kernel (`fibo_r`, `Ack`, etc.) in 100% safe Rust.
//!
//! Runs pure-integer untyped plain self-recursive functions on a flat `Vec<i64>`
//! register buffer and compact 8-byte [`RecFrame`] stack — zero tag checks,
//! zero `TAG_HEAP`, zero `fill(TAG_UNINIT)`, and fused `CallSelf1Sub`!
//! Any overflow, frame-limit hit, or work-budget yield materializes all active
//! frames cleanly back into `Vm::registers` and `Vm::call_stack` at the exact
//! original bytecode IP.

use phera_bytecode::Reg;
use phera_value::Value;

use crate::calls::call_stack::CallStack;
use crate::calls::execution_loop::Position;
use crate::calls::frame::Frame;
use crate::calls::registers::Registers;
use crate::code_table::FunctionIndex;
use crate::fast_tier::{
    CMP_EQ, CMP_GE, CMP_GT, CMP_IDENT, CMP_LE, CMP_LT, CMP_NE, CMP_NOT_IDENT, FastExit, FastOp, Op,
};

/// 8-byte compact recursion frame (3× smaller than `FastFrame`).
#[derive(Clone, Copy)]
pub(crate) struct RecFrame {
    pub(crate) base: u32,
    pub(crate) ret_dst: u8,
    pub(crate) ret_ip: u16,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ROp {
    BranchLtImm,
    BranchLeImm,
    BranchGtImm,
    BranchGeImm,
    BranchEqImm,
    BranchNeImm,
    BranchCmp,
    Jump,
    AddI64,
    SubI64,
    MulI64,
    AddImm,
    SubImm,
    MulImm,
    LoadInt,
    Mov,
    /// Fused `SubImm(t, src, imm) + CallSelf1(dst, t)` (`fibo_r($n - 1)`, `fibo_r($n - 2)`).
    CallSelf1Sub,
    CallSelf1,
    CallSelf2,
    RetImm,
    RetReg,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct RecOp {
    pub(crate) op: ROp,
    pub(crate) a: u8,
    pub(crate) b: u8,
    pub(crate) c: u8,
    pub(crate) imm: i32,
}

const _: () = assert!(std::mem::size_of::<RecOp>() == 8);

impl RecOp {
    #[inline(always)]
    const fn new(op: ROp, a: u8, b: u8, c: u8, imm: i32) -> Self {
        Self { op, a, b, c, imm }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecKernel {
    /// Arbitrary pure-`i64` self-recursive function executed by the flat `RecOp` loop.
    General,
    /// Specialized zero-dispatch iterative kernel for 1-arg binary-branching recurrences:
    /// `if cmp(n, lim) { return base } else { return f(n - step1) <op> f(n - step2) }`.
    UnaryBinOp {
        cmp: u8,
        lim: i64,
        base_is_param: bool,
        base_val: i64,
        step1: i64,
        step2: i64,
        combine: ROp,
        call1_dst: u8,
        call1_ret_ip: u16,
        call2_dst: u8,
        call2_ret_ip: u16,
        combine_ip: u16,
    },
    /// Specialized zero-dispatch iterative kernel for 2-arg nested recurrences:
    /// `if m == 0 { return n + c0 } if n == 0 { return f(m - s0, c1) } return f(m - s0, f(m, n - s1))`.
    BinaryNested {
        c0: i64,
        c1: i64,
        s0: i64,
        s1: i64,
        outer_m_reg: u8,
        inner_ret_dst: u8,
        inner_call_ret_ip: u16,
    },
}

pub(crate) struct IntRecFunc {
    pub(crate) ops: Box<[RecOp]>,
    pub(crate) bc_ip: Box<[u32]>,
    pub(crate) ret_bc_ip: Box<[u32]>,
    pub(crate) reg_count: u8,
    pub(crate) param_count: u8,
    pub(crate) kernel: RecKernel,
}

/// Try to compile a plain untyped self-recursive `FastOp` function with
/// `local_count == param_count` into an [`IntRecFunc`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn try_compile_int_rec(
    ops: &[FastOp],
    bc_ips: &[u32],
    reg_count: u8,
    local_count: u8,
    param_count: u8,
    is_plain: bool,
    takes_values_only: bool,
    typed_params: u64,
    has_return_mask: bool,
) -> Option<IntRecFunc> {
    if !is_plain
        || !takes_values_only
        || typed_params != 0
        || has_return_mask
        || param_count == 0
        || param_count > 4
        || local_count != param_count
        || reg_count == 0
        || reg_count > 32
        || ops.len() > 256
    {
        return None;
    }

    let mut has_self_call = false;
    let mut is_target = vec![false; ops.len() + 1];
    for &op in ops {
        if matches!(op.op, Op::CallSelf1 | Op::CallSelf2) {
            has_self_call = true;
        }
        if let Some(t) = crate::fast_tier::scalar_loop::fast_op_branch_target(op)
            && t < is_target.len()
        {
            is_target[t] = true;
        }
    }
    if !has_self_call {
        return None;
    }

    let mut rops: Vec<RecOp> = Vec::with_capacity(ops.len());
    let mut r_bc_ip: Vec<u32> = Vec::with_capacity(ops.len());
    let mut r_ret_bc_ip: Vec<u32> = Vec::with_capacity(ops.len());
    let mut fast_to_rec = vec![u16::MAX; ops.len() + 1];
    let mut patches: Vec<(usize, usize, PatchKind)> = Vec::new();

    #[derive(Clone, Copy)]
    enum PatchKind {
        Imm,
        Bc15When(bool),
    }

    let mut i = 0usize;
    while i < ops.len() {
        let pos = rops.len();
        fast_to_rec[i] = pos as u16;
        let op = ops[i];
        let next = (i + 1 < ops.len() && !is_target[i + 1]).then(|| ops[i + 1]);

        // Fuse `SubImm(t, src, k) + CallSelf1(dst, t)` -> `CallSelf1Sub(dst, src, k)`!
        if op.op == Op::SubImm
            && op.a >= local_count
            && let Some(n) = next
            && n.op == Op::CallSelf1
            && n.b == op.a
        {
            rops.push(RecOp::new(ROp::CallSelf1Sub, n.a, op.b, 0, op.imm));
            r_bc_ip.push(bc_ips[i]);
            r_ret_bc_ip.push(bc_ips[i + 1] + 1);
            i += 2;
            continue;
        }

        let cur_bc = bc_ips[i];
        let cur_ret_bc = cur_bc + 1;
        match op.op {
            Op::BranchLtImm
            | Op::BranchLeImm
            | Op::BranchGtImm
            | Op::BranchGeImm
            | Op::BranchEqImm
            | Op::BranchNeImm
            | Op::BranchIdentImm
            | Op::BranchNotIdentImm => {
                let rop = match op.op {
                    Op::BranchLtImm => ROp::BranchLtImm,
                    Op::BranchLeImm => ROp::BranchLeImm,
                    Op::BranchGtImm => ROp::BranchGtImm,
                    Op::BranchGeImm => ROp::BranchGeImm,
                    Op::BranchEqImm | Op::BranchIdentImm => ROp::BranchEqImm,
                    Op::BranchNeImm | Op::BranchNotIdentImm => ROp::BranchNeImm,
                    _ => unreachable!(),
                };
                let when = (op.c & 0x80) != 0;
                let target = (op.b as usize) | (((op.c & 0x7f) as usize) << 8);
                rops.push(RecOp::new(rop, op.a, 0, 0, op.imm));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
                patches.push((pos, target, PatchKind::Bc15When(when)));
            }
            Op::BranchCmp => {
                rops.push(RecOp::new(ROp::BranchCmp, op.a, op.b, op.c, 0));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
                patches.push((pos, op.imm as usize, PatchKind::Imm));
            }
            Op::Jump => {
                rops.push(RecOp::new(ROp::Jump, 0, 0, 0, 0));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
                patches.push((pos, op.imm as usize, PatchKind::Imm));
            }
            Op::AddNum => {
                rops.push(RecOp::new(ROp::AddI64, op.a, op.b, op.c, 0));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::SubNum => {
                rops.push(RecOp::new(ROp::SubI64, op.a, op.b, op.c, 0));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::MulNum => {
                rops.push(RecOp::new(ROp::MulI64, op.a, op.b, op.c, 0));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::AddAssign => {
                rops.push(RecOp::new(ROp::AddI64, op.a, op.a, op.b, 0));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::SubAssign => {
                rops.push(RecOp::new(ROp::SubI64, op.a, op.a, op.b, 0));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::MulAssign => {
                rops.push(RecOp::new(ROp::MulI64, op.a, op.a, op.b, 0));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::AddImm => {
                rops.push(RecOp::new(ROp::AddImm, op.a, op.b, 0, op.imm));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::SubImm => {
                rops.push(RecOp::new(ROp::SubImm, op.a, op.b, 0, op.imm));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::MulImm => {
                rops.push(RecOp::new(ROp::MulImm, op.a, op.b, 0, op.imm));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::LoadInt => {
                rops.push(RecOp::new(ROp::LoadInt, op.a, 0, 0, op.imm));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::Move | Op::Assign | Op::AssignTake => {
                rops.push(RecOp::new(ROp::Mov, op.a, op.b, 0, 0));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::Clear => {}
            Op::CallSelf1 => {
                rops.push(RecOp::new(ROp::CallSelf1, op.a, op.b, 0, 0));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::CallSelf2 => {
                rops.push(RecOp::new(ROp::CallSelf2, op.a, op.b, op.c, 0));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::RetIntImm => {
                rops.push(RecOp::new(ROp::RetImm, 0, 0, 0, op.imm));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::ReturnReg => {
                rops.push(RecOp::new(ROp::RetReg, op.a, 0, 0, 0));
                r_bc_ip.push(cur_bc);
                r_ret_bc_ip.push(cur_ret_bc);
            }
            Op::ReturnNull
                if !is_target[i]
                    && i > 0
                    && matches!(ops[i - 1].op, Op::ReturnReg | Op::RetIntImm | Op::Jump) =>
            {
                // Unreachable trailing implicit `return null;` emitted by compiler.
            }
            _ => return None,
        }
        i += 1;
    }
    fast_to_rec[ops.len()] = rops.len() as u16;

    for (pos, fast_target, kind) in patches {
        let rec_target = fast_to_rec.get(fast_target).copied()?;
        if rec_target == u16::MAX {
            return None;
        }
        match kind {
            PatchKind::Imm => rops[pos].imm = i32::from(rec_target),
            PatchKind::Bc15When(when) => {
                rops[pos].b = (rec_target & 0xff) as u8;
                rops[pos].c = (((rec_target >> 8) & 0x7f) as u8) | if when { 0x80 } else { 0 };
            }
        }
    }

    let kernel = detect_rec_kernel(&rops, param_count);
    Some(IntRecFunc {
        ops: rops.into_boxed_slice(),
        bc_ip: r_bc_ip.into_boxed_slice(),
        ret_bc_ip: r_ret_bc_ip.into_boxed_slice(),
        reg_count,
        param_count,
        kernel,
    })
}

/// Detect general zero-dispatch iterative kernels from the lowered `RecOp` slice.
fn detect_rec_kernel(ops: &[RecOp], param_count: u8) -> RecKernel {
    if param_count == 1
        && let Some(k) = detect_unary_binop(ops)
    {
        return k;
    }
    if param_count == 2
        && let Some(k) = detect_binary_nested(ops)
    {
        return k;
    }
    RecKernel::General
}

fn detect_unary_binop(ops: &[RecOp]) -> Option<RecKernel> {
    // Shape A (standard `if ($n < lim) return base; return f($n - s1) op f($n - s2);`
    // and ternary `return ($n < lim) ? base : f($n - s1) op f($n - s2);`):
    //   0: BranchLtImm|BranchLeImm(a=0, imm=lim, when=false -> target=2)
    //   1: RetImm(base_val) | RetReg(0)
    //   2: CallSelf1Sub(dst=d1, src=0, imm=s1)
    //   3: CallSelf1Sub(dst=d2, src=0, imm=s2)
    //   4: AddI64|SubI64|MulI64(dst=dr, b=d1, c=d2)
    //   5: RetReg(dr)
    if ops.len() != 6 {
        return None;
    }
    let br = ops[0];
    if br.a != 0 || !matches!(br.op, ROp::BranchLtImm | ROp::BranchLeImm) {
        return None;
    }
    let cmp = if br.op == ROp::BranchLtImm {
        CMP_LT
    } else {
        CMP_LE
    };
    let when = (br.c & 0x80) != 0;
    let target = (br.b as usize) | (((br.c & 0x7f) as usize) << 8);
    let (base_idx, rec_start) = if !when && target == 2 {
        (1usize, 2usize)
    } else if when && target == 5 {
        (5usize, 1usize)
    } else {
        return None;
    };
    let (base_is_param, base_val) = match ops[base_idx] {
        RecOp {
            op: ROp::RetImm,
            imm,
            ..
        } => (false, i64::from(imm)),
        RecOp {
            op: ROp::RetReg,
            a: 0,
            ..
        } => (true, 0),
        _ => return None,
    };
    let c1 = ops[rec_start];
    let c2 = ops[rec_start + 1];
    let comb = ops[rec_start + 2];
    let ret = ops[rec_start + 3];
    if c1.op != ROp::CallSelf1Sub
        || c1.b != 0
        || c2.op != ROp::CallSelf1Sub
        || c2.b != 0
        || c1.a == 0
        || c2.a == 0
        || c1.a == c2.a
        || !matches!(comb.op, ROp::AddI64 | ROp::SubI64 | ROp::MulI64)
        || comb.b != c1.a
        || comb.c != c2.a
        || ret.op != ROp::RetReg
        || ret.a != comb.a
    {
        return None;
    }
    Some(RecKernel::UnaryBinOp {
        cmp,
        lim: i64::from(br.imm),
        base_is_param,
        base_val,
        step1: i64::from(c1.imm),
        step2: i64::from(c2.imm),
        combine: comb.op,
        call1_dst: c1.a,
        call1_ret_ip: (rec_start + 1) as u16,
        call2_dst: c2.a,
        call2_ret_ip: (rec_start + 2) as u16,
        combine_ip: (rec_start + 2) as u16,
    })
}

fn detect_binary_nested(ops: &[RecOp]) -> Option<RecKernel> {
    // General 2-arg nested recurrence (`Ack` family):
    //   0: BranchEqImm(a=0, imm=0, when=false -> p1)
    //   1: AddImm(dst=t, b=1, imm=c0)
    //   2: RetReg(t)
    //   [optional dead Jump(p1)]
    //  p1: BranchEqImm(a=1, imm=0, when=false -> p2)
    //  p1+1: SubImm(dst=t0, b=0, imm=s0)
    //  p1+2: LoadInt(dst=t1, imm=c1)
    //  p1+3: CallSelf2(dst=tr, b=t0, c=t1)
    //  p1+4: RetReg(tr)
    //   [optional dead Jump(p2)]
    //  p2: SubImm(dst=t2, b=0, imm=s0)
    //   [optional Mov(tm, 0)]
    //  ...: SubImm(dst=t3, b=1, imm=s1)
    //  ...: CallSelf2(dst=t4, b=0|tm, c=t3)
    //  ...: CallSelf2(dst=t5, b=t2, c=t4)
    //  ...: RetReg(t5)
    let br0 = *ops.first()?;
    let p1 = (br0.b as usize) | (((br0.c & 0x7f) as usize) << 8);
    if br0.op != ROp::BranchEqImm
        || br0.a != 0
        || br0.imm != 0
        || (br0.c & 0x80) != 0
        || !(p1 == 3 || (p1 == 4 && ops.get(3).is_some_and(|o| o.op == ROp::Jump)))
    {
        return None;
    }
    let add0 = *ops.get(1)?;
    let ret0 = *ops.get(2)?;
    if add0.op != ROp::AddImm || add0.b != 1 || ret0.op != ROp::RetReg || ret0.a != add0.a {
        return None;
    }

    let br1 = *ops.get(p1)?;
    let p2 = (br1.b as usize) | (((br1.c & 0x7f) as usize) << 8);
    if br1.op != ROp::BranchEqImm
        || br1.a != 1
        || br1.imm != 0
        || (br1.c & 0x80) != 0
        || !(p2 == p1 + 5 || (p2 == p1 + 6 && ops.get(p1 + 5).is_some_and(|o| o.op == ROp::Jump)))
    {
        return None;
    }
    let sub1 = *ops.get(p1 + 1)?;
    let ld1 = *ops.get(p1 + 2)?;
    let call1 = *ops.get(p1 + 3)?;
    let ret1 = *ops.get(p1 + 4)?;
    if sub1.op != ROp::SubImm
        || sub1.b != 0
        || ld1.op != ROp::LoadInt
        || call1.op != ROp::CallSelf2
        || call1.b != sub1.a
        || call1.c != ld1.a
        || ret1.op != ROp::RetReg
        || ret1.a != call1.a
    {
        return None;
    }

    let sub_om = *ops.get(p2)?;
    let (m_arg_reg, q) = match ops.get(p2 + 1) {
        Some(&RecOp {
            op: ROp::Mov,
            a,
            b: 0,
            ..
        }) => (a, p2 + 2),
        _ => (0u8, p2 + 1),
    };
    if q + 4 != ops.len() {
        return None;
    }
    let sub_in = ops[q];
    let inner_call = ops[q + 1];
    let outer_call = ops[q + 2];
    let ret2 = ops[q + 3];
    if sub_om.op != ROp::SubImm
        || sub_om.b != 0
        || sub_om.imm != sub1.imm
        || sub_in.op != ROp::SubImm
        || sub_in.b != 1
        || inner_call.op != ROp::CallSelf2
        || inner_call.b != m_arg_reg
        || inner_call.c != sub_in.a
        || outer_call.op != ROp::CallSelf2
        || outer_call.b != sub_om.a
        || outer_call.c != inner_call.a
        || ret2.op != ROp::RetReg
        || ret2.a != outer_call.a
        || sub_om.a == inner_call.a
    {
        return None;
    }
    Some(RecKernel::BinaryNested {
        c0: i64::from(add0.imm),
        c1: i64::from(ld1.imm),
        s0: i64::from(sub1.imm),
        s1: i64::from(sub_in.imm),
        outer_m_reg: sub_om.a,
        inner_ret_dst: inner_call.a,
        inner_call_ret_ip: (q + 2) as u16,
    })
}

/// Run a pure-`i64` self-recursive entry frame directly from `Vm::registers`
/// without boxing/unboxing or loading `FastStack.tags/bits`.
#[inline(never)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_int_rec_entry(
    rec: &IntRecFunc,
    fn_idx: FunctionIndex,
    stack: &mut CallStack,
    registers: &mut Registers,
    rec_regs: &mut Vec<i64>,
    rec_frames: &mut Vec<RecFrame>,
    entry_frame: Frame,
    max_frames: usize,
    budget_start: usize,
    work_io: &mut usize,
    executed_io: &mut u64,
) -> Option<FastExit> {
    let entry_vm_base = entry_frame.base();
    let param_count = rec.param_count as usize;
    let stride = rec.reg_count as usize;

    // Verify all entry parameters are `Value::Int` BEFORE mutating anything.
    let mut entry_args = [0i64; 4];
    for (p, slot) in entry_args.iter_mut().enumerate().take(param_count) {
        match registers.window(entry_vm_base + p, 1) {
            [Some(Value::Int(i))] => *slot = *i,
            _ => return None,
        }
    }

    let caller_vm = *stack.caller()?;
    let destination = entry_frame.destination()?;

    match rec.kernel {
        RecKernel::UnaryBinOp {
            cmp,
            lim,
            base_is_param,
            base_val,
            step1,
            step2,
            combine,
            call1_dst,
            call1_ret_ip,
            call2_dst,
            call2_ret_ip,
            combine_ip,
        } => {
            return Some(run_unary_binop(
                rec,
                fn_idx,
                stack,
                registers,
                rec_regs,
                rec_frames,
                entry_frame,
                caller_vm,
                destination,
                entry_args[0],
                max_frames,
                budget_start,
                work_io,
                executed_io,
                cmp,
                lim,
                base_is_param,
                base_val,
                step1,
                step2,
                combine,
                call1_dst,
                call1_ret_ip,
                call2_dst,
                call2_ret_ip,
                combine_ip,
            ));
        }
        RecKernel::BinaryNested {
            c0,
            c1,
            s0,
            s1,
            outer_m_reg,
            inner_ret_dst,
            inner_call_ret_ip,
        } => {
            return Some(run_binary_nested(
                rec,
                fn_idx,
                stack,
                registers,
                rec_regs,
                rec_frames,
                entry_frame,
                caller_vm,
                destination,
                entry_args[0],
                entry_args[1],
                max_frames,
                budget_start,
                work_io,
                executed_io,
                c0,
                c1,
                s0,
                s1,
                outer_m_reg,
                inner_ret_dst,
                inner_call_ret_ip,
            ));
        }
        RecKernel::General => {}
    }

    // Pre-size flat buffers ONCE so the inner loop has zero re-slicing, zero
    // `try_into().expect()`, and zero `Vec::push` capacity checks!
    let cap_frames = max_frames.min(4096);
    let needed_regs = (cap_frames + 2) * stride + 32;
    if rec_regs.len() < needed_regs {
        rec_regs.resize(needed_regs, 0);
    }
    rec_frames.clear();
    if rec_frames.capacity() < cap_frames + 1 {
        rec_frames.reserve(cap_frames + 1 - rec_frames.capacity());
    }
    rec_frames.resize(
        cap_frames + 1,
        RecFrame {
            base: 0,
            ret_dst: 0,
            ret_ip: 0,
        },
    );
    rec_regs[..param_count].copy_from_slice(&entry_args[..param_count]);

    let ops = &rec.ops[..];
    let regs = &mut rec_regs[..needed_regs];
    let frames = &mut rec_frames[..cap_frames + 1];
    let mut sp: usize = 0;
    let mut base: usize = 0;
    let mut ip: usize = 0;
    let mut budget = budget_start.max(1);
    let mut yield_now = false;

    while let Some(&op) = ops.get(ip) {
        match op.op {
            ROp::BranchLtImm
            | ROp::BranchLeImm
            | ROp::BranchGtImm
            | ROp::BranchGeImm
            | ROp::BranchEqImm
            | ROp::BranchNeImm => {
                let a = regs[base + (op.a as usize)];
                let imm = i64::from(op.imm);
                let res = match op.op {
                    ROp::BranchLtImm => a < imm,
                    ROp::BranchLeImm => a <= imm,
                    ROp::BranchGtImm => a > imm,
                    ROp::BranchGeImm => a >= imm,
                    ROp::BranchEqImm => a == imm,
                    ROp::BranchNeImm => a != imm,
                    _ => unreachable!(),
                };
                let when = (op.c & 0x80) != 0;
                if res == when {
                    let target = (op.b as usize) | (((op.c & 0x7f) as usize) << 8);
                    if target <= ip {
                        budget -= 1;
                        if budget == 0 {
                            ip = target;
                            yield_now = true;
                            break;
                        }
                    }
                    ip = target;
                } else {
                    ip += 1;
                }
            }
            ROp::BranchCmp => {
                let a = regs[base + (op.b as usize)];
                let b = regs[base + (op.c as usize)];
                let cmp = op.a & 0x0f;
                let when = (op.a & 0x10) != 0;
                let res = match cmp {
                    CMP_EQ | CMP_IDENT => a == b,
                    CMP_NE | CMP_NOT_IDENT => a != b,
                    CMP_LT => a < b,
                    CMP_LE => a <= b,
                    CMP_GT => a > b,
                    CMP_GE => a >= b,
                    _ => break,
                };
                if res == when {
                    let target = op.imm as usize;
                    if target <= ip {
                        budget -= 1;
                        if budget == 0 {
                            ip = target;
                            yield_now = true;
                            break;
                        }
                    }
                    ip = target;
                } else {
                    ip += 1;
                }
            }
            ROp::Jump => {
                let target = op.imm as usize;
                if target <= ip {
                    budget -= 1;
                    if budget == 0 {
                        ip = target;
                        yield_now = true;
                        break;
                    }
                }
                ip = target;
            }
            ROp::AddI64 => {
                let Some(s) =
                    regs[base + (op.b as usize)].checked_add(regs[base + (op.c as usize)])
                else {
                    break;
                };
                regs[base + (op.a as usize)] = s;
                ip += 1;
            }
            ROp::SubI64 => {
                let Some(s) =
                    regs[base + (op.b as usize)].checked_sub(regs[base + (op.c as usize)])
                else {
                    break;
                };
                regs[base + (op.a as usize)] = s;
                ip += 1;
            }
            ROp::MulI64 => {
                let Some(s) =
                    regs[base + (op.b as usize)].checked_mul(regs[base + (op.c as usize)])
                else {
                    break;
                };
                regs[base + (op.a as usize)] = s;
                ip += 1;
            }
            ROp::AddImm => {
                let Some(s) = regs[base + (op.b as usize)].checked_add(i64::from(op.imm)) else {
                    break;
                };
                regs[base + (op.a as usize)] = s;
                ip += 1;
            }
            ROp::SubImm => {
                let Some(s) = regs[base + (op.b as usize)].checked_sub(i64::from(op.imm)) else {
                    break;
                };
                regs[base + (op.a as usize)] = s;
                ip += 1;
            }
            ROp::MulImm => {
                let Some(s) = regs[base + (op.b as usize)].checked_mul(i64::from(op.imm)) else {
                    break;
                };
                regs[base + (op.a as usize)] = s;
                ip += 1;
            }
            ROp::LoadInt => {
                regs[base + (op.a as usize)] = i64::from(op.imm);
                ip += 1;
            }
            ROp::Mov => {
                regs[base + (op.a as usize)] = regs[base + (op.b as usize)];
                ip += 1;
            }
            ROp::CallSelf1Sub => {
                let Some(arg0) = regs[base + (op.b as usize)].checked_sub(i64::from(op.imm)) else {
                    break;
                };
                if sp >= cap_frames {
                    break;
                }
                frames[sp] = RecFrame {
                    base: base as u32,
                    ret_dst: op.a,
                    ret_ip: (ip + 1) as u16,
                };
                sp += 1;
                base += stride;
                regs[base] = arg0;
                ip = 0;
                budget -= 1;
                if budget == 0 {
                    yield_now = true;
                    break;
                }
            }
            ROp::CallSelf1 => {
                if sp >= cap_frames {
                    break;
                }
                let arg0 = regs[base + (op.b as usize)];
                frames[sp] = RecFrame {
                    base: base as u32,
                    ret_dst: op.a,
                    ret_ip: (ip + 1) as u16,
                };
                sp += 1;
                base += stride;
                regs[base] = arg0;
                ip = 0;
                budget -= 1;
                if budget == 0 {
                    yield_now = true;
                    break;
                }
            }
            ROp::CallSelf2 => {
                if sp >= cap_frames {
                    break;
                }
                let arg0 = regs[base + (op.b as usize)];
                let arg1 = regs[base + (op.c as usize)];
                frames[sp] = RecFrame {
                    base: base as u32,
                    ret_dst: op.a,
                    ret_ip: (ip + 1) as u16,
                };
                sp += 1;
                base += stride;
                regs[base] = arg0;
                regs[base + 1] = arg1;
                ip = 0;
                budget -= 1;
                if budget == 0 {
                    yield_now = true;
                    break;
                }
            }
            ROp::RetImm | ROp::RetReg => {
                let ret = if op.op == ROp::RetImm {
                    i64::from(op.imm)
                } else {
                    regs[base + (op.a as usize)]
                };
                if sp > 0 {
                    sp -= 1;
                    let caller = frames[sp];
                    base = caller.base as usize;
                    regs[base + caller.ret_dst as usize] = ret;
                    ip = caller.ret_ip as usize;
                    continue;
                }
                // Entry frame returned cleanly!
                let spent = budget_start.saturating_sub(budget);
                *work_io = (*work_io).saturating_add(spent);
                *executed_io += spent.max(1) as u64;
                rec_frames.clear();
                stack.pop_for_return();
                registers.truncate(entry_frame.start());
                registers.put(caller_vm.base() + destination.index(), Value::Int(ret));
                return Some(FastExit::Returned(Position {
                    frame: caller_vm,
                    ip: caller_vm.ip(),
                }));
            }
        }
    }

    rec_frames.truncate(sp);
    // Materialize active `RecFrame`s cleanly back to `Vm::registers` + `CallStack`!
    Some(materialize_int_rec(
        rec,
        fn_idx,
        stack,
        registers,
        rec_regs,
        rec_frames,
        entry_frame,
        base,
        ip,
        yield_now,
        budget_start.saturating_sub(budget),
        work_io,
        executed_io,
    ))
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn run_unary_binop(
    rec: &IntRecFunc,
    fn_idx: FunctionIndex,
    stack: &mut CallStack,
    registers: &mut Registers,
    rec_regs: &mut Vec<i64>,
    rec_frames: &mut Vec<RecFrame>,
    entry_frame: Frame,
    caller_vm: Frame,
    destination: Reg,
    entry_n: i64,
    max_frames: usize,
    budget_start: usize,
    work_io: &mut usize,
    executed_io: &mut u64,
    cmp: u8,
    lim: i64,
    base_is_param: bool,
    base_val: i64,
    step1: i64,
    step2: i64,
    combine: ROp,
    call1_dst: u8,
    call1_ret_ip: u16,
    call2_dst: u8,
    call2_ret_ip: u16,
    combine_ip: u16,
) -> FastExit {
    enum Pause {
        CallAtTop { n: i64, ip: usize },
        Call2Overflow { ret1: i64 },
        CombineOverflow { lhs: i64, rhs: i64 },
    }

    let max_d = max_frames.min(255);
    let mut st_n = [0i64; 256];
    let mut st_acc = [0i64; 256];
    let mut st_phase = [0u8; 256];
    let mut sp: usize = 0;
    let mut n = entry_n;
    // Each zero-dispatch iterative step is ~8-10x faster than a Tier-1 bytecode
    // step; scale internal step budget by 8x (~1 ms per yield) and charge `spent / 8`.
    let step_budget_start = budget_start.max(1).saturating_mul(8);
    let mut budget = step_budget_start;
    let mut yield_now = false;

    let pause = 'run: loop {
        let is_base = if cmp == CMP_LT { n < lim } else { n <= lim };
        let mut ret = if is_base {
            if base_is_param { n } else { base_val }
        } else {
            if sp >= max_d {
                break 'run Pause::CallAtTop { n, ip: 0 };
            }
            let Some(arg1) = n.checked_sub(step1) else {
                break 'run Pause::CallAtTop {
                    n,
                    ip: (call1_ret_ip as usize).saturating_sub(1),
                };
            };
            st_n[sp] = n;
            st_phase[sp] = 0;
            sp += 1;
            n = arg1;
            budget -= 1;
            if budget == 0 {
                yield_now = true;
                break 'run Pause::CallAtTop { n, ip: 0 };
            }
            continue 'run;
        };

        loop {
            if sp == 0 {
                let raw_spent = step_budget_start.saturating_sub(budget);
                let spent = raw_spent >> 3;
                *work_io = (*work_io).saturating_add(spent);
                *executed_io += raw_spent.max(1) as u64;
                stack.pop_for_return();
                registers.truncate(entry_frame.start());
                registers.put(caller_vm.base() + destination.index(), Value::Int(ret));
                return FastExit::Returned(Position {
                    frame: caller_vm,
                    ip: caller_vm.ip(),
                });
            }
            let top = sp - 1;
            if st_phase[top] == 0 {
                let cur_n = st_n[top];
                let Some(arg2) = cur_n.checked_sub(step2) else {
                    break 'run Pause::Call2Overflow { ret1: ret };
                };
                st_acc[top] = ret;
                st_phase[top] = 1;
                n = arg2;
                budget -= 1;
                if budget == 0 {
                    yield_now = true;
                    break 'run Pause::CallAtTop { n, ip: 0 };
                }
                continue 'run;
            }
            let lhs = st_acc[top];
            let combined = match combine {
                ROp::AddI64 => lhs.checked_add(ret),
                ROp::SubI64 => lhs.checked_sub(ret),
                ROp::MulI64 => lhs.checked_mul(ret),
                _ => None,
            };
            let Some(val) = combined else {
                break 'run Pause::CombineOverflow { lhs, rhs: ret };
            };
            ret = val;
            sp = top;
        }
    };

    // Reconstruct exact `rec_regs` + `rec_frames` for materialization.
    let stride = rec.reg_count as usize;
    let needed = (sp + 2) * stride;
    if rec_regs.len() < needed {
        rec_regs.resize(needed, 0);
    }
    rec_frames.clear();
    let (saved_count, cur_base, cur_ip) = match pause {
        Pause::CallAtTop { n: cur_n, ip } => {
            let cur_base = sp * stride;
            rec_regs[cur_base..cur_base + stride].fill(0);
            rec_regs[cur_base] = cur_n;
            (sp, cur_base, ip)
        }
        Pause::Call2Overflow { ret1 } => {
            let top = sp - 1;
            let cur_base = top * stride;
            rec_regs[cur_base..cur_base + stride].fill(0);
            rec_regs[cur_base] = st_n[top];
            rec_regs[cur_base + call1_dst as usize] = ret1;
            (top, cur_base, (call2_ret_ip as usize).saturating_sub(1))
        }
        Pause::CombineOverflow { lhs, rhs } => {
            let top = sp - 1;
            let cur_base = top * stride;
            rec_regs[cur_base..cur_base + stride].fill(0);
            rec_regs[cur_base] = st_n[top];
            rec_regs[cur_base + call1_dst as usize] = lhs;
            rec_regs[cur_base + call2_dst as usize] = rhs;
            (top, cur_base, combine_ip as usize)
        }
    };
    for k in 0..saved_count {
        let f_base = k * stride;
        rec_regs[f_base..f_base + stride].fill(0);
        rec_regs[f_base] = st_n[k];
        if st_phase[k] == 0 {
            rec_frames.push(RecFrame {
                base: f_base as u32,
                ret_dst: call1_dst,
                ret_ip: call1_ret_ip,
            });
        } else {
            rec_regs[f_base + call1_dst as usize] = st_acc[k];
            rec_frames.push(RecFrame {
                base: f_base as u32,
                ret_dst: call2_dst,
                ret_ip: call2_ret_ip,
            });
        }
    }
    materialize_int_rec(
        rec,
        fn_idx,
        stack,
        registers,
        rec_regs,
        rec_frames,
        entry_frame,
        cur_base,
        cur_ip,
        yield_now,
        step_budget_start.saturating_sub(budget) >> 3,
        work_io,
        executed_io,
    )
}

/// Pure inline subtree evaluator for `Op::CallSelf1` inside `FastVm` so
/// post-yield sibling calls still execute inside the zero-dispatch kernel!
/// Returns `Some((result, work_spent))` when completed within budget/depth,
/// or `None` with zero side effects if it would overflow/yield/hit depth.
#[inline(never)]
pub(crate) fn try_eval_int_rec_1(
    rec: &IntRecFunc,
    entry_n: i64,
    max_frames: usize,
    budget_work: usize,
) -> Option<(i64, usize)> {
    let RecKernel::UnaryBinOp {
        cmp,
        lim,
        base_is_param,
        base_val,
        step1,
        step2,
        combine,
        ..
    } = rec.kernel
    else {
        return None;
    };
    let max_d = max_frames.min(255);
    let mut st_n = [0i64; 256];
    let mut st_acc = [0i64; 256];
    let mut st_phase = [0u8; 256];
    let mut sp: usize = 0;
    let mut n = entry_n;
    let step_budget_start = budget_work.max(1).saturating_mul(8);
    let mut budget = step_budget_start;

    loop {
        let is_base = if cmp == CMP_LT { n < lim } else { n <= lim };
        let mut ret = if is_base {
            if base_is_param { n } else { base_val }
        } else {
            if sp >= max_d {
                return None;
            }
            let arg1 = n.checked_sub(step1)?;
            st_n[sp] = n;
            st_phase[sp] = 0;
            sp += 1;
            n = arg1;
            budget -= 1;
            if budget == 0 {
                return None;
            }
            continue;
        };
        loop {
            if sp == 0 {
                let spent = step_budget_start.saturating_sub(budget) >> 3;
                return Some((ret, spent));
            }
            let top = sp - 1;
            if st_phase[top] == 0 {
                let arg2 = st_n[top].checked_sub(step2)?;
                st_acc[top] = ret;
                st_phase[top] = 1;
                n = arg2;
                budget -= 1;
                if budget == 0 {
                    return None;
                }
                break;
            }
            let lhs = st_acc[top];
            ret = match combine {
                ROp::AddI64 => lhs.checked_add(ret)?,
                ROp::SubI64 => lhs.checked_sub(ret)?,
                ROp::MulI64 => lhs.checked_mul(ret)?,
                _ => return None,
            };
            sp = top;
        }
    }
}

/// Pure inline subtree evaluator for `Op::CallSelf2` inside `FastVm` so
/// post-yield sibling calls still execute inside the zero-dispatch kernel!
#[inline(never)]
pub(crate) fn try_eval_int_rec_2(
    rec: &IntRecFunc,
    entry_m: i64,
    entry_n: i64,
    max_frames: usize,
    budget_work: usize,
) -> Option<(i64, usize)> {
    let RecKernel::BinaryNested { c0, c1, s0, s1, .. } = rec.kernel else {
        return None;
    };
    let max_d = max_frames.min(2047);
    let mut st_m = [0i64; 2048];
    let mut sp: usize = 0;
    let mut m = entry_m;
    let mut n = entry_n;
    let step_budget_start = budget_work.max(1).saturating_mul(8);
    let mut budget = step_budget_start;

    loop {
        if m == 0 {
            let res = n.checked_add(c0)?;
            if sp == 0 {
                let spent = step_budget_start.saturating_sub(budget) >> 3;
                return Some((res, spent));
            }
            sp -= 1;
            m = st_m[sp];
            n = res;
        } else if n == 0 {
            m = m.checked_sub(s0)?;
            n = c1;
        } else {
            if sp >= max_d {
                return None;
            }
            let outer_m = m.checked_sub(s0)?;
            let inner_n = n.checked_sub(s1)?;
            st_m[sp] = outer_m;
            sp += 1;
            n = inner_n;
        }
        budget -= 1;
        if budget == 0 {
            return None;
        }
    }
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn run_binary_nested(
    rec: &IntRecFunc,
    fn_idx: FunctionIndex,
    stack: &mut CallStack,
    registers: &mut Registers,
    rec_regs: &mut Vec<i64>,
    rec_frames: &mut Vec<RecFrame>,
    entry_frame: Frame,
    caller_vm: Frame,
    destination: Reg,
    entry_m: i64,
    entry_n: i64,
    max_frames: usize,
    budget_start: usize,
    work_io: &mut usize,
    executed_io: &mut u64,
    c0: i64,
    c1: i64,
    s0: i64,
    s1: i64,
    outer_m_reg: u8,
    inner_ret_dst: u8,
    inner_call_ret_ip: u16,
) -> FastExit {
    let max_d = max_frames.min(2047);
    let mut st_m = [0i64; 2048];
    let mut sp: usize = 0;
    let mut m = entry_m;
    let mut n = entry_n;
    let step_budget_start = budget_start.max(1).saturating_mul(8);
    let mut budget = step_budget_start;
    let mut yield_now = false;

    loop {
        if m == 0 {
            let Some(res) = n.checked_add(c0) else {
                break;
            };
            if sp == 0 {
                let raw_spent = step_budget_start.saturating_sub(budget);
                let spent = raw_spent >> 3;
                *work_io = (*work_io).saturating_add(spent);
                *executed_io += raw_spent.max(1) as u64;
                stack.pop_for_return();
                registers.truncate(entry_frame.start());
                registers.put(caller_vm.base() + destination.index(), Value::Int(res));
                return FastExit::Returned(Position {
                    frame: caller_vm,
                    ip: caller_vm.ip(),
                });
            }
            sp -= 1;
            m = st_m[sp];
            n = res;
        } else if n == 0 {
            let Some(nm) = m.checked_sub(s0) else {
                break;
            };
            m = nm;
            n = c1;
        } else {
            if sp >= max_d {
                break;
            }
            let Some(outer_m) = m.checked_sub(s0) else {
                break;
            };
            let Some(inner_n) = n.checked_sub(s1) else {
                break;
            };
            st_m[sp] = outer_m;
            sp += 1;
            n = inner_n;
        }
        budget -= 1;
        if budget == 0 {
            yield_now = true;
            break;
        }
    }

    // Reconstruct exact `rec_regs` + `rec_frames` for materialization.
    let stride = rec.reg_count as usize;
    let needed = (sp + 2) * stride;
    if rec_regs.len() < needed {
        rec_regs.resize(needed, 0);
    }
    rec_frames.clear();
    for (k, &outer_m) in st_m.iter().enumerate().take(sp) {
        let f_base = k * stride;
        rec_regs[f_base..f_base + stride].fill(0);
        rec_regs[f_base] = outer_m.wrapping_add(s0);
        rec_regs[f_base + outer_m_reg as usize] = outer_m;
        rec_frames.push(RecFrame {
            base: f_base as u32,
            ret_dst: inner_ret_dst,
            ret_ip: inner_call_ret_ip,
        });
    }
    let cur_base = sp * stride;
    rec_regs[cur_base..cur_base + stride].fill(0);
    rec_regs[cur_base] = m;
    rec_regs[cur_base + 1] = n;
    materialize_int_rec(
        rec,
        fn_idx,
        stack,
        registers,
        rec_regs,
        rec_frames,
        entry_frame,
        cur_base,
        0,
        yield_now,
        step_budget_start.saturating_sub(budget) >> 3,
        work_io,
        executed_io,
    )
}

#[cold]
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn materialize_int_rec(
    rec: &IntRecFunc,
    fn_idx: FunctionIndex,
    stack: &mut CallStack,
    registers: &mut Registers,
    rec_regs: &[i64],
    rec_frames: &mut Vec<RecFrame>,
    entry_frame: Frame,
    cur_base: usize,
    cur_ip: usize,
    yield_now: bool,
    spent: usize,
    work_io: &mut usize,
    executed_io: &mut u64,
) -> FastExit {
    let stride = rec.reg_count as usize;
    let cur_bc_ip = rec.bc_ip.get(cur_ip).copied().unwrap_or(0) as usize;

    // 1. Store entry frame's registers back to its VM window.
    for (r, &val) in rec_regs.iter().enumerate().take(stride) {
        registers.put(entry_frame.base() + r, Value::Int(val));
    }

    // 2. Materialize nested frames in bottom-up order.
    let final_frame = if rec_frames.is_empty() {
        let mut frame = entry_frame;
        frame.set_ip(cur_bc_ip);
        if let Some(current) = stack.current_mut() {
            current.set_ip(cur_bc_ip);
        }
        frame
    } else {
        let saved = std::mem::take(rec_frames);
        let entry_ret_bc = rec.ret_bc_ip[(saved[0].ret_ip as usize).saturating_sub(1)] as usize;
        if let Some(current) = stack.current_mut() {
            current.set_ip(entry_ret_bc);
        }
        for k in 1..saved.len() {
            let f = saved[k];
            let prev_dst = saved[k - 1].ret_dst;
            let vm_base = registers.len();
            registers.grow_to(vm_base + stride);
            for r in 0..stride {
                registers.put(vm_base + r, Value::Int(rec_regs[f.base as usize + r]));
            }
            let mut vm_frame = Frame::new(fn_idx, vm_base, Some(Reg(u16::from(prev_dst))));
            let ret_bc = rec.ret_bc_ip[(f.ret_ip as usize).saturating_sub(1)] as usize;
            vm_frame.set_ip(ret_bc);
            stack.push(vm_frame);
        }
        let last_dst = saved.last().expect("non-empty").ret_dst;
        let inner_vm_base = registers.len();
        registers.grow_to(inner_vm_base + stride);
        for r in 0..stride {
            registers.put(inner_vm_base + r, Value::Int(rec_regs[cur_base + r]));
        }
        let mut inner_frame = Frame::new(fn_idx, inner_vm_base, Some(Reg(u16::from(last_dst))));
        inner_frame.set_ip(cur_bc_ip);
        stack.push(inner_frame);
        inner_frame
    };

    if yield_now {
        *work_io = 0;
    } else {
        *work_io = (*work_io).saturating_add(spent);
    }
    *executed_io += spent.max(1) as u64;
    FastExit::Deopt {
        position: Position {
            frame: final_frame,
            ip: cur_bc_ip,
        },
        yield_now,
        disable_fn: None,
    }
}
