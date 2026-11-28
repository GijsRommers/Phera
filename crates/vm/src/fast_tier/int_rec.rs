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
use crate::calls::prepared_call::CallKind;
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
    /// Specialized zero-dispatch iterative kernel for 1-arg binary-branching recurrences.
    UnaryBinOp(UnaryBinOp),
    /// Specialized zero-dispatch iterative kernel for 2-arg nested recurrences.
    BinaryNested(BinaryNested),
}

/// `if cmp(n, lim) { return base } else { return f(n - step1) <op> f(n - step2) }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UnaryBinOp {
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
}

/// A [`BinaryNested`] frame waiting for its inner call `f($m, $n - s1)`:
/// the `$m - s0` it then passes to its outer call, and its own `$n`, which
/// a materialised frame shows (`debug_backtrace`, exception traces).
#[derive(Clone, Copy)]
pub(crate) struct WaitingCall {
    outer_m: i64,
    n: i64,
}

/// `if m == 0 { return n + c0 } if n == 0 { return f(m - s0, c1) } return f(m - s0, f(m, n - s1))`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BinaryNested {
    c0: i64,
    c1: i64,
    s0: i64,
    s1: i64,
    outer_m_reg: u8,
    inner_ret_dst: u8,
    inner_call_ret_ip: u16,
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
    Some(RecKernel::UnaryBinOp(UnaryBinOp {
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
    }))
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
    Some(RecKernel::BinaryNested(BinaryNested {
        c0: i64::from(add0.imm),
        c1: i64::from(ld1.imm),
        s0: i64::from(sub1.imm),
        s1: i64::from(sub_in.imm),
        outer_m_reg: sub_om.a,
        inner_ret_dst: inner_call.a,
        inner_call_ret_ip: (q + 2) as u16,
    }))
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
    rec_stack: &mut Vec<WaitingCall>,
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

    if !matches!(rec.kernel, RecKernel::General) {
        let entry = KernelEntry {
            rec,
            fn_idx,
            stack,
            registers,
            rec_regs,
            rec_frames,
            entry_frame,
            caller_vm,
            destination,
            max_frames,
            budget_start,
            work_io,
            executed_io,
        };
        return Some(match rec.kernel {
            RecKernel::UnaryBinOp(k) => {
                run_unary_binop(entry, k, &[], UnaryStart::Call(entry_args[0]))
            }
            RecKernel::BinaryNested(k) => {
                rec_stack.clear();
                run_binary_nested(entry, k, rec_stack, entry_args[0], entry_args[1])
            }
            RecKernel::General => unreachable!(),
        });
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

/// What a specialised kernel needs from the VM around it: the frame it runs
/// for (the bottom one when it resumes several), where that frame's result
/// goes, and the scratch and accounting a materialisation uses.
struct KernelEntry<'a> {
    rec: &'a IntRecFunc,
    fn_idx: FunctionIndex,
    stack: &'a mut CallStack,
    registers: &'a mut Registers,
    rec_regs: &'a mut Vec<i64>,
    rec_frames: &'a mut Vec<RecFrame>,
    entry_frame: Frame,
    caller_vm: Frame,
    destination: Reg,
    max_frames: usize,
    budget_start: usize,
    work_io: &'a mut usize,
    executed_io: &'a mut u64,
}

impl KernelEntry<'_> {
    /// Each kernel step is about eight times cheaper than a Tier-1
    /// instruction, so a kernel gets eight steps per unit of work budget
    /// (about 1 ms between yields) and is charged `steps / 8`.
    fn step_budget(&self) -> usize {
        self.budget_start.max(1).saturating_mul(8)
    }

    /// The entry frame returned `ret` after `steps` kernel steps: leave it
    /// and hand `ret` to its caller.
    fn finish(self, ret: i64, steps: usize) -> FastExit {
        *self.work_io = (*self.work_io).saturating_add(steps >> 3);
        *self.executed_io += steps.max(1) as u64;
        self.stack.pop_for_return();
        self.registers.truncate(self.entry_frame.start());
        self.registers.put(
            self.caller_vm.base() + self.destination.index(),
            Value::Int(ret),
        );
        FastExit::Returned(Position {
            frame: self.caller_vm,
            ip: self.caller_vm.ip(),
        })
    }

    /// Hands the frames rebuilt in `rec_regs` / `rec_frames` back to the VM.
    fn materialize(
        self,
        cur_base: usize,
        cur_ip: usize,
        yield_now: bool,
        steps: usize,
    ) -> FastExit {
        materialize_int_rec(
            self.rec,
            self.fn_idx,
            self.stack,
            self.registers,
            self.rec_regs,
            self.rec_frames,
            self.entry_frame,
            cur_base,
            cur_ip,
            yield_now,
            steps >> 3,
            self.work_io,
            self.executed_io,
        )
    }
}

/// How a [`UnaryBinOp`] kernel run begins: with a call of `f($n)`, or with
/// a value returned to the innermost open frame (when it resumes frames).
#[derive(Clone, Copy)]
enum UnaryStart {
    Call(i64),
    Return(i64),
}

/// The deepest a [`UnaryBinOp`] kernel goes: its frames live in fixed
/// arrays on the Rust stack.
const UNARY_MAX_DEPTH: usize = 255;

/// Runs a [`UnaryBinOp`] recursion. `open` are frames already waiting for a
/// self-call, bottom first: each one's `$n`, and the first call's result
/// once that call has returned (it then waits for the second).
#[inline(never)]
fn run_unary_binop(
    entry: KernelEntry<'_>,
    k: UnaryBinOp,
    open: &[(i64, Option<i64>)],
    start: UnaryStart,
) -> FastExit {
    enum Pause {
        CallAtTop { n: i64, ip: usize },
        Call2Overflow { ret1: i64 },
        CombineOverflow { lhs: i64, rhs: i64 },
    }

    let max_d = entry.max_frames.min(UNARY_MAX_DEPTH);
    let mut st_n = [0i64; UNARY_MAX_DEPTH + 1];
    let mut st_acc = [0i64; UNARY_MAX_DEPTH + 1];
    let mut st_phase = [0u8; UNARY_MAX_DEPTH + 1];
    for (slot, &(n, first)) in open.iter().enumerate().take(max_d) {
        st_n[slot] = n;
        if let Some(acc) = first {
            st_acc[slot] = acc;
            st_phase[slot] = 1;
        }
    }
    let mut sp = open.len().min(max_d);
    let step_budget_start = entry.step_budget();
    let mut budget = step_budget_start;
    let mut yield_now = false;
    let (mut n, mut ret, mut returning) = match start {
        UnaryStart::Call(n) => (n, 0, false),
        UnaryStart::Return(ret) => (0, ret, true),
    };

    let pause = 'run: loop {
        if !returning {
            let is_base = if k.cmp == CMP_LT {
                n < k.lim
            } else {
                n <= k.lim
            };
            if !is_base {
                if sp >= max_d {
                    break 'run Pause::CallAtTop { n, ip: 0 };
                }
                let Some(arg1) = n.checked_sub(k.step1) else {
                    break 'run Pause::CallAtTop {
                        n,
                        ip: (k.call1_ret_ip as usize).saturating_sub(1),
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
            }
            ret = if k.base_is_param { n } else { k.base_val };
        }
        returning = false;

        loop {
            if sp == 0 {
                return entry.finish(ret, step_budget_start.saturating_sub(budget));
            }
            let top = sp - 1;
            if st_phase[top] == 0 {
                let Some(arg2) = st_n[top].checked_sub(k.step2) else {
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
            let combined = match k.combine {
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
    let rec_regs = &mut *entry.rec_regs;
    let rec_frames = &mut *entry.rec_frames;
    let stride = entry.rec.reg_count as usize;
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
            rec_regs[cur_base + k.call1_dst as usize] = ret1;
            (top, cur_base, (k.call2_ret_ip as usize).saturating_sub(1))
        }
        Pause::CombineOverflow { lhs, rhs } => {
            let top = sp - 1;
            let cur_base = top * stride;
            rec_regs[cur_base..cur_base + stride].fill(0);
            rec_regs[cur_base] = st_n[top];
            rec_regs[cur_base + k.call1_dst as usize] = lhs;
            rec_regs[cur_base + k.call2_dst as usize] = rhs;
            (top, cur_base, k.combine_ip as usize)
        }
    };
    for slot in 0..saved_count {
        let f_base = slot * stride;
        rec_regs[f_base..f_base + stride].fill(0);
        rec_regs[f_base] = st_n[slot];
        if st_phase[slot] == 0 {
            rec_frames.push(RecFrame {
                base: f_base as u32,
                ret_dst: k.call1_dst,
                ret_ip: k.call1_ret_ip,
            });
        } else {
            rec_regs[f_base + k.call1_dst as usize] = st_acc[slot];
            rec_frames.push(RecFrame {
                base: f_base as u32,
                ret_dst: k.call2_dst,
                ret_ip: k.call2_ret_ip,
            });
        }
    }
    let steps = step_budget_start.saturating_sub(budget);
    entry.materialize(cur_base, cur_ip, yield_now, steps)
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
    let RecKernel::UnaryBinOp(UnaryBinOp {
        cmp,
        lim,
        base_is_param,
        base_val,
        step1,
        step2,
        combine,
        ..
    }) = rec.kernel
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
/// `waiting` is reusable scratch for the frames waiting on an inner call.
#[inline(never)]
pub(crate) fn try_eval_int_rec_2(
    rec: &IntRecFunc,
    waiting: &mut Vec<WaitingCall>,
    entry_m: i64,
    entry_n: i64,
    max_frames: usize,
    budget_work: usize,
) -> Option<(i64, usize)> {
    let RecKernel::BinaryNested(k) = rec.kernel else {
        return None;
    };
    waiting.clear();
    let mut m = entry_m;
    let mut n = entry_n;
    let step_budget_start = budget_work.max(1).saturating_mul(8);
    let mut budget = step_budget_start;

    loop {
        if m == 0 {
            let res = n.checked_add(k.c0)?;
            let Some(WaitingCall { outer_m, .. }) = waiting.pop() else {
                let spent = step_budget_start.saturating_sub(budget) >> 3;
                return Some((res, spent));
            };
            m = outer_m;
            n = res;
        } else if n == 0 {
            m = m.checked_sub(k.s0)?;
            n = k.c1;
        } else {
            if waiting.len() >= max_frames {
                return None;
            }
            let outer_m = m.checked_sub(k.s0)?;
            let inner_n = n.checked_sub(k.s1)?;
            waiting.push(WaitingCall { outer_m, n });
            n = inner_n;
        }
        budget -= 1;
        if budget == 0 {
            return None;
        }
    }
}

/// Runs a [`BinaryNested`] recursion from a call of `f(m, n)`. `waiting`
/// holds the frames already waiting for an inner call, bottom first.
#[inline(never)]
fn run_binary_nested(
    entry: KernelEntry<'_>,
    k: BinaryNested,
    waiting: &mut Vec<WaitingCall>,
    entry_m: i64,
    entry_n: i64,
) -> FastExit {
    let max_d = entry.max_frames;
    let mut m = entry_m;
    let mut n = entry_n;
    let step_budget_start = entry.step_budget();
    let mut budget = step_budget_start;
    let mut yield_now = false;

    loop {
        if m == 0 {
            let Some(res) = n.checked_add(k.c0) else {
                break;
            };
            let Some(WaitingCall { outer_m, .. }) = waiting.pop() else {
                return entry.finish(res, step_budget_start.saturating_sub(budget));
            };
            m = outer_m;
            n = res;
        } else if n == 0 {
            let Some(nm) = m.checked_sub(k.s0) else {
                break;
            };
            m = nm;
            n = k.c1;
        } else {
            if waiting.len() >= max_d {
                break;
            }
            let Some(outer_m) = m.checked_sub(k.s0) else {
                break;
            };
            let Some(inner_n) = n.checked_sub(k.s1) else {
                break;
            };
            waiting.push(WaitingCall { outer_m, n });
            n = inner_n;
        }
        budget -= 1;
        if budget == 0 {
            yield_now = true;
            break;
        }
    }

    // Reconstruct exact `rec_regs` + `rec_frames` for materialization.
    let rec_regs = &mut *entry.rec_regs;
    let rec_frames = &mut *entry.rec_frames;
    let stride = entry.rec.reg_count as usize;
    let sp = waiting.len();
    let needed = (sp + 2) * stride;
    if rec_regs.len() < needed {
        rec_regs.resize(needed, 0);
    }
    rec_frames.clear();
    for (slot, &WaitingCall { outer_m, n }) in waiting.iter().enumerate() {
        let f_base = slot * stride;
        rec_regs[f_base..f_base + stride].fill(0);
        rec_regs[f_base] = outer_m.wrapping_add(k.s0);
        rec_regs[f_base + 1] = n;
        rec_regs[f_base + k.outer_m_reg as usize] = outer_m;
        rec_frames.push(RecFrame {
            base: f_base as u32,
            ret_dst: k.inner_ret_dst,
            ret_ip: k.inner_call_ret_ip,
        });
    }
    let cur_base = sp * stride;
    rec_regs[cur_base..cur_base + stride].fill(0);
    rec_regs[cur_base] = m;
    rec_regs[cur_base + 1] = n;
    let steps = step_budget_start.saturating_sub(budget);
    entry.materialize(cur_base, 0, yield_now, steps)
}

/// What a frame of a specialised kernel's function is waiting for, read
/// from the instruction it continues at, with the register its callee's
/// result goes to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Parked {
    /// A [`UnaryBinOp`] frame waiting for its first self-call.
    First,
    /// A [`UnaryBinOp`] frame waiting for its second self-call.
    Second,
    /// A [`BinaryNested`] frame waiting for its inner self-call.
    Inner,
}

impl IntRecFunc {
    /// The bytecode instruction a frame continues at after the self-call
    /// whose return lands at `RecOp` index `ret_ip`.
    fn resume_bc_ip(&self, ret_ip: u16) -> Option<usize> {
        let call = (ret_ip as usize).checked_sub(1)?;
        self.ret_bc_ip.get(call).map(|&ip| ip as usize)
    }

    /// Whether a frame continuing at `bc_ip` waits for one of the kernel's
    /// self-calls, and the register that call's result goes to.
    fn parked_at(&self, bc_ip: usize) -> Option<(Parked, u8)> {
        match self.kernel {
            RecKernel::UnaryBinOp(k) => {
                if self.resume_bc_ip(k.call1_ret_ip) == Some(bc_ip) {
                    Some((Parked::First, k.call1_dst))
                } else if self.resume_bc_ip(k.call2_ret_ip) == Some(bc_ip) {
                    Some((Parked::Second, k.call2_dst))
                } else {
                    None
                }
            }
            RecKernel::BinaryNested(k) => (self.resume_bc_ip(k.inner_call_ret_ip) == Some(bc_ip))
                .then_some((Parked::Inner, k.inner_ret_dst)),
            RecKernel::General => None,
        }
    }

    /// What a parked frame keeps for the kernel: `$n` and the first call's
    /// result for a [`UnaryBinOp`] frame, the outer call's `$m` and `$n`
    /// for a [`BinaryNested`] one. `None` when a register holds something
    /// other than an int.
    fn parked_state(
        &self,
        registers: &Registers,
        base: usize,
        parked: Parked,
    ) -> Option<(i64, i64)> {
        let int = |reg: u8| match registers.window(base + reg as usize, 1) {
            [Some(Value::Int(value))] => Some(*value),
            _ => None,
        };
        match (self.kernel, parked) {
            (RecKernel::UnaryBinOp(_), Parked::First) => Some((int(0)?, 0)),
            (RecKernel::UnaryBinOp(k), Parked::Second) => Some((int(0)?, int(k.call1_dst)?)),
            (RecKernel::BinaryNested(k), Parked::Inner) => Some((int(k.outer_m_reg)?, int(1)?)),
            _ => None,
        }
    }
}

/// A frame the kernel may take over: a plain call of `function` with a
/// result register, nothing in the side table and every argument passed.
fn plain_frame_of(frame: &Frame, function: FunctionIndex) -> bool {
    frame.function == function
        && frame.kind() == CallKind::Function
        && frame.destination().is_some()
        && !frame.has_extras()
        && frame.passed().is_none()
}

/// Resumes a specialised kernel at a frame waiting for one of its
/// self-calls: after a yield materialised the kernel's frames, the machine
/// ran the innermost one, and its caller now continues here. The frames of
/// the same function waiting below it are taken back into the kernel too,
/// so the rest of the recursion runs in the kernel again instead of one
/// frame at a time. `None`, with nothing changed, when the frames do not
/// fit.
#[inline(never)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn resume_int_rec(
    rec: &IntRecFunc,
    stack: &mut CallStack,
    registers: &mut Registers,
    rec_regs: &mut Vec<i64>,
    rec_frames: &mut Vec<RecFrame>,
    rec_stack: &mut Vec<WaitingCall>,
    top_ip: usize,
    stop: usize,
    frame_limit: usize,
    budget_start: usize,
    work_io: &mut usize,
    executed_io: &mut u64,
) -> Option<FastExit> {
    let top_depth = stack.depth().checked_sub(1)?;
    let top = *stack.current()?;
    let function = top.function;
    if top_depth <= stop || !plain_frame_of(&top, function) {
        return None;
    }
    let (top_parked, top_dst) = rec.parked_at(top_ip)?;
    let returned = match registers.window(top.base() + top_dst as usize, 1) {
        [Some(Value::Int(value))] => *value,
        _ => return None,
    };

    // Walk down the waiting frames, innermost first: each must wait for
    // the call the frame above it is. A unary kernel keeps its frames in
    // fixed arrays, so it takes at most that many.
    let most = match rec.kernel {
        RecKernel::UnaryBinOp(_) => UNARY_MAX_DEPTH,
        _ => usize::MAX,
    };
    let mut parked = vec![(
        top_parked,
        rec.parked_state(registers, top.base(), top_parked)?,
    )];
    let mut bottom = top_depth;
    while bottom - 1 > stop && parked.len() < most {
        let callee = *stack.at(bottom)?;
        let frame = *stack.at(bottom - 1)?;
        if !plain_frame_of(&frame, function) {
            break;
        }
        let Some((kind, dst)) = rec.parked_at(frame.ip()) else {
            break;
        };
        if callee.destination() != Some(Reg(u16::from(dst))) {
            break;
        }
        let Some(state) = rec.parked_state(registers, frame.base(), kind) else {
            break;
        };
        parked.push((kind, state));
        bottom -= 1;
    }
    parked.reverse();
    let entry_frame = *stack.at(bottom)?;
    let caller_vm = *stack.at(bottom.checked_sub(1)?)?;
    let destination = entry_frame.destination()?;
    if stack.has_calls_from(entry_frame.start()) {
        return None;
    }
    let max_frames = frame_limit.saturating_sub(bottom + 1);
    if parked.len() > max_frames {
        return None;
    }

    // The kernel now holds the waiting frames: leave all but the bottom one.
    for _ in bottom..top_depth {
        stack.pop_for_return();
    }
    registers.truncate(entry_frame.base() + rec.reg_count as usize);
    let entry = KernelEntry {
        rec,
        fn_idx: function,
        stack,
        registers,
        rec_regs,
        rec_frames,
        entry_frame,
        caller_vm,
        destination,
        max_frames,
        budget_start,
        work_io,
        executed_io,
    };
    Some(match rec.kernel {
        RecKernel::UnaryBinOp(k) => {
            let open: Vec<(i64, Option<i64>)> = parked
                .iter()
                .map(|&(kind, (n, first))| (n, (kind == Parked::Second).then_some(first)))
                .collect();
            run_unary_binop(entry, k, &open, UnaryStart::Return(returned))
        }
        RecKernel::BinaryNested(k) => {
            // The innermost frame makes its outer call with the value it got.
            let (waiting, innermost) = parked.split_at(parked.len() - 1);
            let (_, (outer_m, _)) = innermost[0];
            rec_stack.clear();
            rec_stack.extend(
                waiting
                    .iter()
                    .map(|&(_, (outer_m, n))| WaitingCall { outer_m, n }),
            );
            run_binary_nested(entry, k, rec_stack, outer_m, returned)
        }
        RecKernel::General => return None,
    })
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
