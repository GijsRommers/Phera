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

pub(crate) struct IntRecFunc {
    pub(crate) ops: Box<[RecOp]>,
    pub(crate) bc_ip: Box<[u32]>,
    pub(crate) ret_bc_ip: Box<[u32]>,
    pub(crate) reg_count: u8,
    pub(crate) param_count: u8,
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

    Some(IntRecFunc {
        ops: rops.into_boxed_slice(),
        bc_ip: r_bc_ip.into_boxed_slice(),
        ret_bc_ip: r_ret_bc_ip.into_boxed_slice(),
        reg_count,
        param_count,
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
