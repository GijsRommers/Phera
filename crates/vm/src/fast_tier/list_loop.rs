//! Hoisted single-list array + scalar loop compiler & executor (100% safe Rust).
//!
//! For any PHP loop whose only heap accesses are integer-indexed reads/writes
//! on a single array register (`heapsort_r($n, &$ra)`, `sieve($n)`, sorting,
//! searching, DP, prefix sums, etc.), compiles the loop (including nested loops
//! and internal `if`/`else` branches) into a compact [`ListLoop`] that:
//! - Borrows the array's `Storage::List` / `Storage::OffsetList` `&mut [Option<Value>]`
//!   slice ONCE at loop entry (including through `Value::Reference` for `&$ra`!)
//! - Monomorphizes over `i64` vs `f64` array element types via [`ListElem`]
//! - Runs all index/counter arithmetic unboxed in `[i64; 32]` and array values
//!   unboxed in `[E; 16]` with dirty-bitmask write-back so uninitialized locals
//!   stay exact on early exit, yield, or fallback
//! - Falls back cleanly to `FastVm` before mutating on out-of-bounds, type
//!   mismatch, reference element, overflow, or budget exhaustion.

use std::rc::Rc;

use phera_value::{PhpArray, Value};

use crate::fast_tier::scalar_loop::fast_op_branch_target;
use crate::fast_tier::{
    CMP_EQ, CMP_GE, CMP_GT, CMP_IDENT, CMP_LE, CMP_LT, CMP_NE, CMP_NOT_IDENT, FastOp, Op,
    TAG_FLOAT, TAG_HEAP, TAG_INT,
};

pub(crate) const MAX_LIST_INT_SLOTS: usize = 32;
pub(crate) const MAX_LIST_ELEM_SLOTS: usize = 16;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LOp {
    // Hoisted slice access on the single borrowed `(start, slots)` slice
    ListGet,      // ev[a] = slots[iv[b] - start]
    ListGetPlus1, // ev[a] = slots[iv[b] + 1 - start]
    ListSet,      // slots[iv[a] - start] = ev[b]
    ListCopy,     // slots[iv[a] - start] = slots[iv[b] - start]
    FillStep,     // for k = iv[a]; k <= lim; k += iv[b] { slots[k - start] = E::from_i32(val) }

    // Element-typed moves / constants / comparisons
    MovElem,
    LoadElemImm,
    CmpElemBranch,    // a=flags(0x80=ext), b=lhs_e, c=rhs_e, imm=target
    CmpElemImmBranch, // a=(slot&15)|(cmp<<4), b=t_lo, c=(t_hi&0x3f)|(ext<<6)|(when<<7), imm=rhs

    // Unboxed i64 arithmetic & moves
    LoadI64,
    MovI64,
    AddI64,
    SubI64,
    MulI64,
    AddImmI64,
    SubImmI64,
    ShlImmI64,
    ShrImmI64,
    IncI64,
    DecI64,
    PreIncI64,
    PostIncI64,
    PreDecI64,
    PostDecI64,

    // Control flow & loop latches (local or external)
    Jump,            // c=0 local, c=1 external; imm=target
    ExitToFast,      // always exit to fast_ip = imm (for Return inside loop)
    CmpI64Branch,    // a=flags(0x80=ext), b=lhs_i, c=rhs_i, imm=target
    CmpI64ImmBranch, // a=(slot&31)|(cmp<<5), b=t_lo, c=(t_hi&0x3f)|(ext<<6)|(when<<7), imm=rhs
    IncLoopLtImm,    // a=slot, b=t_lo, c=t_hi(0x80=ext), imm=lim
    IncLoopLeImm,
    DecLoopGeImm,
    DecLoopGtImm,
    IncLoopLtReg, // a=slot, b=lim_slot, c=ext, imm=target
    IncLoopLeReg,
    DecLoopGeReg,
    DecLoopGtReg,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ListOp {
    pub(crate) op: LOp,
    pub(crate) a: u8,
    pub(crate) b: u8,
    pub(crate) c: u8,
    pub(crate) imm: i32,
}

const _: () = assert!(std::mem::size_of::<ListOp>() == 8);

impl ListOp {
    #[inline(always)]
    const fn new(op: LOp, a: u8, b: u8, c: u8, imm: i32) -> Self {
        Self { op, a, b, c, imm }
    }
}

pub(crate) struct ListLoop {
    pub(crate) ops: Box<[ListOp]>,
    pub(crate) fast_ips: Box<[u16]>,
    pub(crate) fallthrough_fast_ip: u16,
    pub(crate) arr_reg: u8,
    pub(crate) int_regs: Box<[u8]>,
    pub(crate) elem_regs: Box<[u8]>,
    pub(crate) live_in_ints: Box<[u8]>,
    pub(crate) live_in_elems: Box<[u8]>,
    pub(crate) written_ints: Box<[u8]>,
    pub(crate) written_elems: Box<[u8]>,
}

pub(crate) enum ListExit {
    Exit {
        next_fast_ip: usize,
        spent: usize,
    },
    Yield {
        resume_fast_ip: usize,
    },
    Fallback {
        fallback_fast_ip: usize,
        spent: usize,
    },
}

trait ListElem: Copy {
    const TAG: u8;
    const ZERO: Self;
    fn from_value(v: &Value) -> Option<Self>;
    fn to_value(self) -> Value;
    fn from_bits(bits: u64) -> Self;
    fn to_bits(self) -> u64;
    fn from_i32(x: i32) -> Self;
    fn cmp(code: u8, a: Self, b: Self) -> bool;
}

impl ListElem for i64 {
    const TAG: u8 = TAG_INT;
    const ZERO: Self = 0;
    #[inline(always)]
    fn from_value(v: &Value) -> Option<Self> {
        match *v {
            Value::Int(i) => Some(i),
            _ => None,
        }
    }
    #[inline(always)]
    fn to_value(self) -> Value {
        Value::Int(self)
    }
    #[inline(always)]
    fn from_bits(bits: u64) -> Self {
        bits as i64
    }
    #[inline(always)]
    fn to_bits(self) -> u64 {
        self as u64
    }
    #[inline(always)]
    fn from_i32(x: i32) -> Self {
        i64::from(x)
    }
    #[inline(always)]
    fn cmp(code: u8, a: Self, b: Self) -> bool {
        match code {
            CMP_EQ | CMP_IDENT => a == b,
            CMP_NE | CMP_NOT_IDENT => a != b,
            CMP_LT => a < b,
            CMP_LE => a <= b,
            CMP_GT => a > b,
            CMP_GE => a >= b,
            _ => false,
        }
    }
}

impl ListElem for f64 {
    const TAG: u8 = TAG_FLOAT;
    const ZERO: Self = 0.0;
    #[inline(always)]
    fn from_value(v: &Value) -> Option<Self> {
        match *v {
            Value::Float(f) => Some(f),
            _ => None,
        }
    }
    #[inline(always)]
    fn to_value(self) -> Value {
        Value::Float(self)
    }
    #[inline(always)]
    fn from_bits(bits: u64) -> Self {
        f64::from_bits(bits)
    }
    #[inline(always)]
    fn to_bits(self) -> u64 {
        self.to_bits()
    }
    #[inline(always)]
    fn from_i32(x: i32) -> Self {
        f64::from(x)
    }
    #[inline(always)]
    fn cmp(code: u8, a: Self, b: Self) -> bool {
        match code {
            CMP_EQ | CMP_IDENT => a == b,
            CMP_NE | CMP_NOT_IDENT => a != b,
            CMP_LT => a < b,
            CMP_LE => a <= b,
            CMP_GT => a > b,
            CMP_GE => a >= b,
            _ => false,
        }
    }
}

/// Entry guard, array borrow (direct or via `Value::Reference`), monomorphized
/// execution, and dirty-only write-back for `Op::EnterListLoop`.
#[inline(never)]
pub(crate) fn try_run_list_loop(
    ll: &ListLoop,
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    heap: &mut [Option<Value>],
    budget: usize,
) -> Option<ListExit> {
    let arr_r = ll.arr_reg as usize;
    if t[arr_r] != TAG_HEAP {
        return None;
    }
    // Verify live-in integer registers and non-heap written integer/elem registers.
    for &slot in ll.live_in_ints.iter() {
        if t[ll.int_regs[slot as usize] as usize] != TAG_INT {
            return None;
        }
    }
    for &slot in ll.written_ints.iter() {
        if t[ll.int_regs[slot as usize] as usize] == TAG_HEAP {
            return None;
        }
    }
    for &slot in ll.written_elems.iter() {
        if t[ll.elem_regs[slot as usize] as usize] == TAG_HEAP {
            return None;
        }
    }

    let heap_slot = v[arr_r] as usize;
    match heap.get_mut(heap_slot) {
        Some(Some(Value::Array(rc))) => run_on_array_rc(ll, rc, t, v, budget),
        Some(Some(Value::Reference(cell))) => {
            let mut guard = cell.try_borrow_mut().ok()?;
            match &mut *guard {
                Value::Array(rc) => run_on_array_rc(ll, rc, t, v, budget),
                _ => None,
            }
        }
        _ => None,
    }
}

fn run_on_array_rc(
    ll: &ListLoop,
    rc: &mut Rc<PhpArray>,
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    budget: usize,
) -> Option<ListExit> {
    let arr = Rc::make_mut(rc);
    let (start, slots) = arr.list_slots_mut()?;
    // Detect element type from first present slot (or first live-in elem).
    let is_float = match slots.iter().find_map(Option::as_ref) {
        Some(Value::Float(_)) => true,
        Some(Value::Int(_)) => false,
        _ => return None,
    };
    if is_float {
        run_list_loop_typed::<f64>(ll, start, slots, t, v, budget)
    } else {
        run_list_loop_typed::<i64>(ll, start, slots, t, v, budget)
    }
}

#[inline(never)]
fn run_list_loop_typed<E: ListElem>(
    ll: &ListLoop,
    start: i64,
    slots: &mut [Option<Value>],
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    budget_start: usize,
) -> Option<ListExit> {
    for &slot in ll.live_in_elems.iter() {
        if t[ll.elem_regs[slot as usize] as usize] != E::TAG {
            return None;
        }
    }

    let mut iv = [0i64; MAX_LIST_INT_SLOTS];
    let mut ev = [E::ZERO; MAX_LIST_ELEM_SLOTS];
    for &slot in ll.live_in_ints.iter() {
        let s = slot as usize;
        iv[s] = v[ll.int_regs[s] as usize] as i64;
    }
    for &slot in ll.live_in_elems.iter() {
        let s = slot as usize;
        ev[s] = E::from_bits(v[ll.elem_regs[s] as usize]);
    }

    let mut dirty_int: u32 = 0;
    let mut dirty_elem: u32 = 0;
    let exit = exec_list_ops::<E>(
        &ll.ops,
        &ll.fast_ips,
        ll.fallthrough_fast_ip as usize,
        start,
        slots,
        &mut iv,
        &mut ev,
        &mut dirty_int,
        &mut dirty_elem,
        budget_start,
    );

    // Write back ONLY registers actually written during this invocation.
    for &slot in ll.written_ints.iter() {
        let s = slot as usize;
        if (dirty_int & (1u32 << s)) != 0 {
            let r = ll.int_regs[s] as usize;
            t[r] = TAG_INT;
            v[r] = iv[s] as u64;
        }
    }
    for &slot in ll.written_elems.iter() {
        let s = slot as usize;
        if (dirty_elem & (1u32 << s)) != 0 {
            let r = ll.elem_regs[s] as usize;
            t[r] = E::TAG;
            v[r] = ev[s].to_bits();
        }
    }

    Some(exit)
}

#[inline(always)]
fn eval_i64_cmp(code: u8, a: i64, b: i64) -> bool {
    match code {
        CMP_EQ | CMP_IDENT => a == b,
        CMP_NE | CMP_NOT_IDENT => a != b,
        CMP_LT => a < b,
        CMP_LE => a <= b,
        CMP_GT => a > b,
        CMP_GE => a >= b,
        _ => false,
    }
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn exec_list_ops<E: ListElem>(
    ops: &[ListOp],
    fast_ips: &[u16],
    fallthrough_fast_ip: usize,
    start: i64,
    slots: &mut [Option<Value>],
    iv: &mut [i64; MAX_LIST_INT_SLOTS],
    ev: &mut [E; MAX_LIST_ELEM_SLOTS],
    dirty_int: &mut u32,
    dirty_elem: &mut u32,
    budget_start: usize,
) -> ListExit {
    let mut ip: usize = 0;
    let mut budget = budget_start.max(1);
    let len = ops.len();
    let slots_len = slots.len();

    while ip < len {
        let op = ops[ip];
        match op.op {
            LOp::ListGet => {
                let off = iv[(op.b & 31) as usize].wrapping_sub(start);
                if off < 0 || (off as usize) >= slots_len {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
                let Some(val) = slots[off as usize].as_ref().and_then(E::from_value) else {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                let dst = (op.a & 15) as usize;
                ev[dst] = val;
                *dirty_elem |= 1u32 << dst;
                ip += 1;
            }
            LOp::ListGetPlus1 => {
                let Some(idx) = iv[(op.b & 31) as usize].checked_add(1) else {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                let off = idx.wrapping_sub(start);
                if off < 0 || (off as usize) >= slots_len {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
                let Some(val) = slots[off as usize].as_ref().and_then(E::from_value) else {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                let dst = (op.a & 15) as usize;
                ev[dst] = val;
                *dirty_elem |= 1u32 << dst;
                ip += 1;
            }
            LOp::ListSet => {
                let off = iv[(op.a & 31) as usize].wrapping_sub(start);
                if off < 0 || (off as usize) >= slots_len {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
                match &mut slots[off as usize] {
                    Some(existing) if !existing.is_reference() => {
                        *existing = ev[(op.b & 15) as usize].to_value();
                    }
                    _ => {
                        return ListExit::Fallback {
                            fallback_fast_ip: fast_ips[ip] as usize,
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                }
                ip += 1;
            }
            LOp::ListCopy => {
                let src_off = iv[(op.b & 31) as usize].wrapping_sub(start);
                let dst_off = iv[(op.a & 31) as usize].wrapping_sub(start);
                if src_off < 0
                    || dst_off < 0
                    || (src_off as usize) >= slots_len
                    || (dst_off as usize) >= slots_len
                {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
                let Some(val) = slots[src_off as usize].as_ref().and_then(E::from_value) else {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                match &mut slots[dst_off as usize] {
                    Some(existing) if !existing.is_reference() => {
                        *existing = val.to_value();
                    }
                    _ => {
                        return ListExit::Fallback {
                            fallback_fast_ip: fast_ips[ip] as usize,
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                }
                ip += 1;
            }
            LOp::FillStep => {
                let k_slot = (op.a & 31) as usize;
                let step_slot = (op.b & 31) as usize;
                let lim = i64::from(op.imm & 0x003f_ffff);
                let fill_val = E::from_i32(i32::from(((op.imm >> 24) & 0xff) as i8)).to_value();
                let step = iv[step_slot];
                let mut k = iv[k_slot];
                loop {
                    let off = k.wrapping_sub(start);
                    if off < 0 || (off as usize) >= slots_len {
                        iv[k_slot] = k;
                        *dirty_int |= 1u32 << k_slot;
                        return ListExit::Fallback {
                            fallback_fast_ip: fast_ips[ip] as usize,
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                    match &mut slots[off as usize] {
                        Some(existing) if !existing.is_reference() => {
                            *existing = fill_val.clone();
                        }
                        _ => {
                            iv[k_slot] = k;
                            *dirty_int |= 1u32 << k_slot;
                            return ListExit::Fallback {
                                fallback_fast_ip: fast_ips[ip] as usize,
                                spent: budget_start.saturating_sub(budget),
                            };
                        }
                    }
                    let Some(next_k) = k.checked_add(step) else {
                        iv[k_slot] = k;
                        *dirty_int |= 1u32 << k_slot;
                        return ListExit::Fallback {
                            fallback_fast_ip: fast_ips[ip] as usize,
                            spent: budget_start.saturating_sub(budget),
                        };
                    };
                    k = next_k;
                    if k > lim {
                        iv[k_slot] = k;
                        *dirty_int |= 1u32 << k_slot;
                        ip += 1;
                        break;
                    }
                    budget -= 1;
                    if budget == 0 {
                        iv[k_slot] = k;
                        *dirty_int |= 1u32 << k_slot;
                        return ListExit::Yield {
                            resume_fast_ip: fast_ips[ip] as usize,
                        };
                    }
                }
            }
            LOp::MovElem => {
                let dst = (op.a & 15) as usize;
                ev[dst] = ev[(op.b & 15) as usize];
                *dirty_elem |= 1u32 << dst;
                ip += 1;
            }
            LOp::LoadElemImm => {
                let dst = (op.a & 15) as usize;
                ev[dst] = E::from_i32(op.imm);
                *dirty_elem |= 1u32 << dst;
                ip += 1;
            }
            LOp::LoadI64 => {
                let dst = (op.a & 31) as usize;
                iv[dst] = i64::from(op.imm);
                *dirty_int |= 1u32 << dst;
                ip += 1;
            }
            LOp::MovI64 => {
                let dst = (op.a & 31) as usize;
                iv[dst] = iv[(op.b & 31) as usize];
                *dirty_int |= 1u32 << dst;
                ip += 1;
            }
            LOp::AddI64 => {
                let Some(s) = iv[(op.b & 31) as usize].checked_add(iv[(op.c & 31) as usize]) else {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                let dst = (op.a & 31) as usize;
                iv[dst] = s;
                *dirty_int |= 1u32 << dst;
                ip += 1;
            }
            LOp::SubI64 => {
                let Some(s) = iv[(op.b & 31) as usize].checked_sub(iv[(op.c & 31) as usize]) else {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                let dst = (op.a & 31) as usize;
                iv[dst] = s;
                *dirty_int |= 1u32 << dst;
                ip += 1;
            }
            LOp::MulI64 => {
                let Some(s) = iv[(op.b & 31) as usize].checked_mul(iv[(op.c & 31) as usize]) else {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                let dst = (op.a & 31) as usize;
                iv[dst] = s;
                *dirty_int |= 1u32 << dst;
                ip += 1;
            }
            LOp::AddImmI64 => {
                let Some(s) = iv[(op.b & 31) as usize].checked_add(i64::from(op.imm)) else {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                let dst = (op.a & 31) as usize;
                iv[dst] = s;
                *dirty_int |= 1u32 << dst;
                ip += 1;
            }
            LOp::SubImmI64 => {
                let Some(s) = iv[(op.b & 31) as usize].checked_sub(i64::from(op.imm)) else {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                let dst = (op.a & 31) as usize;
                iv[dst] = s;
                *dirty_int |= 1u32 << dst;
                ip += 1;
            }
            LOp::ShlImmI64 => {
                let shift = op.imm;
                if shift < 0 {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
                let val = if shift >= 64 {
                    0
                } else {
                    iv[(op.b & 31) as usize].wrapping_shl(shift as u32)
                };
                let dst = (op.a & 31) as usize;
                iv[dst] = val;
                *dirty_int |= 1u32 << dst;
                ip += 1;
            }
            LOp::ShrImmI64 => {
                let shift = op.imm;
                if shift < 0 {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
                let src = iv[(op.b & 31) as usize];
                let val = if shift >= 64 {
                    if src < 0 { -1 } else { 0 }
                } else {
                    src >> (shift as u32)
                };
                let dst = (op.a & 31) as usize;
                iv[dst] = val;
                *dirty_int |= 1u32 << dst;
                ip += 1;
            }
            LOp::IncI64 => {
                let idx = (op.a & 31) as usize;
                let Some(s) = iv[idx].checked_add(1) else {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[idx] = s;
                *dirty_int |= 1u32 << idx;
                ip += 1;
            }
            LOp::DecI64 => {
                let idx = (op.a & 31) as usize;
                let Some(s) = iv[idx].checked_sub(1) else {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[idx] = s;
                *dirty_int |= 1u32 << idx;
                ip += 1;
            }
            LOp::PreIncI64 | LOp::PostIncI64 | LOp::PreDecI64 | LOp::PostDecI64 => {
                let dst = (op.a & 31) as usize;
                let loc = (op.b & 31) as usize;
                let old = iv[loc];
                let next = if matches!(op.op, LOp::PreIncI64 | LOp::PostIncI64) {
                    old.checked_add(1)
                } else {
                    old.checked_sub(1)
                };
                let Some(new_val) = next else {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[loc] = new_val;
                *dirty_int |= 1u32 << loc;
                iv[dst] = if matches!(op.op, LOp::PostIncI64 | LOp::PostDecI64) {
                    old
                } else {
                    new_val
                };
                *dirty_int |= 1u32 << dst;
                ip += 1;
            }
            LOp::ExitToFast => {
                return ListExit::Exit {
                    next_fast_ip: op.imm as usize,
                    spent: budget_start.saturating_sub(budget),
                };
            }
            LOp::Jump => {
                let target = op.imm as usize;
                if op.c != 0 {
                    return ListExit::Exit {
                        next_fast_ip: target,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
                if target <= ip {
                    budget -= 1;
                    if budget == 0 {
                        return ListExit::Yield {
                            resume_fast_ip: fast_ips[target] as usize,
                        };
                    }
                }
                ip = target;
            }
            LOp::CmpI64Branch => {
                let cmp = op.a & 0x0f;
                let when = (op.a & 0x10) != 0;
                if eval_i64_cmp(cmp, iv[(op.b & 31) as usize], iv[(op.c & 31) as usize]) == when {
                    let target = op.imm as usize;
                    if (op.a & 0x80) != 0 {
                        return ListExit::Exit {
                            next_fast_ip: target,
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                    if target <= ip {
                        budget -= 1;
                        if budget == 0 {
                            return ListExit::Yield {
                                resume_fast_ip: fast_ips[target] as usize,
                            };
                        }
                    }
                    ip = target;
                } else {
                    ip += 1;
                }
            }
            LOp::CmpI64ImmBranch => {
                let reg = (op.a & 31) as usize;
                let cmp = (op.a >> 5) & 7;
                let when = (op.c & 0x80) != 0;
                if eval_i64_cmp(cmp, iv[reg], i64::from(op.imm)) == when {
                    let target = (op.b as usize) | (((op.c & 0x3f) as usize) << 8);
                    if (op.c & 0x40) != 0 {
                        return ListExit::Exit {
                            next_fast_ip: target,
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                    if target <= ip {
                        budget -= 1;
                        if budget == 0 {
                            return ListExit::Yield {
                                resume_fast_ip: fast_ips[target] as usize,
                            };
                        }
                    }
                    ip = target;
                } else {
                    ip += 1;
                }
            }
            LOp::CmpElemBranch => {
                let cmp = op.a & 0x0f;
                let when = (op.a & 0x10) != 0;
                if E::cmp(cmp, ev[(op.b & 15) as usize], ev[(op.c & 15) as usize]) == when {
                    let target = op.imm as usize;
                    if (op.a & 0x80) != 0 {
                        return ListExit::Exit {
                            next_fast_ip: target,
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                    if target <= ip {
                        budget -= 1;
                        if budget == 0 {
                            return ListExit::Yield {
                                resume_fast_ip: fast_ips[target] as usize,
                            };
                        }
                    }
                    ip = target;
                } else {
                    ip += 1;
                }
            }
            LOp::CmpElemImmBranch => {
                let reg = (op.a & 15) as usize;
                let cmp = (op.a >> 4) & 7;
                let when = (op.c & 0x80) != 0;
                if E::cmp(cmp, ev[reg], E::from_i32(op.imm)) == when {
                    let target = (op.b as usize) | (((op.c & 0x3f) as usize) << 8);
                    if (op.c & 0x40) != 0 {
                        return ListExit::Exit {
                            next_fast_ip: target,
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                    if target <= ip {
                        budget -= 1;
                        if budget == 0 {
                            return ListExit::Yield {
                                resume_fast_ip: fast_ips[target] as usize,
                            };
                        }
                    }
                    ip = target;
                } else {
                    ip += 1;
                }
            }
            LOp::IncLoopLtImm | LOp::IncLoopLeImm | LOp::DecLoopGeImm | LOp::DecLoopGtImm => {
                let idx = (op.a & 31) as usize;
                let next = if matches!(op.op, LOp::IncLoopLtImm | LOp::IncLoopLeImm) {
                    iv[idx].checked_add(1)
                } else {
                    iv[idx].checked_sub(1)
                };
                let Some(nv) = next else {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[idx] = nv;
                *dirty_int |= 1u32 << idx;
                let lim = i64::from(op.imm);
                let taken = match op.op {
                    LOp::IncLoopLtImm => nv < lim,
                    LOp::IncLoopLeImm => nv <= lim,
                    LOp::DecLoopGeImm => nv >= lim,
                    LOp::DecLoopGtImm => nv > lim,
                    _ => false,
                };
                if taken {
                    let target = (op.b as usize) | (((op.c & 0x7f) as usize) << 8);
                    if (op.c & 0x80) != 0 {
                        return ListExit::Exit {
                            next_fast_ip: target,
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                    if target <= ip {
                        budget -= 1;
                        if budget == 0 {
                            return ListExit::Yield {
                                resume_fast_ip: fast_ips[target] as usize,
                            };
                        }
                    }
                    ip = target;
                } else {
                    ip += 1;
                }
            }
            LOp::IncLoopLtReg | LOp::IncLoopLeReg | LOp::DecLoopGeReg | LOp::DecLoopGtReg => {
                let idx = (op.a & 31) as usize;
                let lim = iv[(op.b & 31) as usize];
                let next = if matches!(op.op, LOp::IncLoopLtReg | LOp::IncLoopLeReg) {
                    iv[idx].checked_add(1)
                } else {
                    iv[idx].checked_sub(1)
                };
                let Some(nv) = next else {
                    return ListExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[idx] = nv;
                *dirty_int |= 1u32 << idx;
                let taken = match op.op {
                    LOp::IncLoopLtReg => nv < lim,
                    LOp::IncLoopLeReg => nv <= lim,
                    LOp::DecLoopGeReg => nv >= lim,
                    LOp::DecLoopGtReg => nv > lim,
                    _ => false,
                };
                if taken {
                    let target = op.imm as usize;
                    if op.c != 0 {
                        return ListExit::Exit {
                            next_fast_ip: target,
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                    if target <= ip {
                        budget -= 1;
                        if budget == 0 {
                            return ListExit::Yield {
                                resume_fast_ip: fast_ips[target] as usize,
                            };
                        }
                    }
                    ip = target;
                } else {
                    ip += 1;
                }
            }
        }
    }

    ListExit::Exit {
        next_fast_ip: fallthrough_fast_ip,
        spent: budget_start.saturating_sub(budget),
    }
}

// =============================================================================
// Compiler: extract `ListLoop`s from lowered `FastOp` streams
// =============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum LKind {
    Unknown,
    Int,
    Elem,
    Conflict,
}

impl LKind {
    fn unify(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unknown, x) | (x, Self::Unknown) => x,
            (a, b) if a == b => a,
            _ => Self::Conflict,
        }
    }
}

fn branch_imm_cmp_code(op: Op) -> Option<u8> {
    match op {
        Op::BranchLtImm => Some(CMP_LT),
        Op::BranchLeImm => Some(CMP_LE),
        Op::BranchGtImm => Some(CMP_GT),
        Op::BranchGeImm => Some(CMP_GE),
        Op::BranchEqImm => Some(CMP_EQ),
        Op::BranchNeImm => Some(CMP_NE),
        Op::BranchIdentImm => Some(CMP_IDENT),
        Op::BranchNotIdentImm => Some(CMP_NOT_IDENT),
        _ => None,
    }
}

/// Try to compile `ops[body_start..=end]` (whose `EnterListLoop` is at
/// `header_ip`, with back-edge to `back_target`) into a single-array [`ListLoop`].
pub(crate) fn try_compile_list_loop(
    ops: &[FastOp],
    body_start: usize,
    end: usize,
    back_target: usize,
    local_count: u8,
) -> Option<ListLoop> {
    if body_start > end || end >= ops.len() || end - body_start > 128 {
        return None;
    }
    let body = &ops[body_start..=end];
    let n = body.len();

    // 1. Identify the single array register `arr_reg` accessed by the loop.
    let mut arr_reg: Option<u8> = None;
    let mut bind_arr = |r: u8| -> bool {
        match arr_reg {
            None => {
                arr_reg = Some(r);
                true
            }
            Some(existing) => existing == r,
        }
    };
    for &op in body {
        match op.op {
            Op::ArrGetInt if op.imm == 0 => {
                if !bind_arr(op.b) {
                    return None;
                }
            }
            Op::ArrSetInt => {
                if !bind_arr(op.a) {
                    return None;
                }
            }
            Op::ArrSetRegStepLoop if (op.imm & (1 << 23)) != 0 => {
                if !bind_arr(op.a) {
                    return None;
                }
            }
            Op::EnterScalarLoop | Op::EnterListLoop => return None,
            _ => {}
        }
    }
    let arr_reg = arr_reg?;

    // 1b. Identify internal branch targets and pre-fuse adjacent single-use temp
    //     array idioms (`AddImm(t, idx, 1) + ArrGetInt(dst, arr, t)` -> `ListGetPlus1`,
    //     `ArrGetInt(t, arr, src_idx) + ArrSetInt(arr, dst_idx, t)` -> `ListCopy`)
    //     so reused temp registers never cause false Int/Elem conflicts!
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Fused {
        None,
        GetPlus1,
        Copy,
        Consumed,
    }
    let mut is_local_target = [false; 130];
    for &op in body {
        if let Some(t) = fast_op_branch_target(op)
            && t >= body_start
            && t <= end
        {
            is_local_target[t - body_start] = true;
        }
    }
    let mut fused = [Fused::None; 130];
    let mut fk = 0usize;
    while fk + 1 < n {
        if !is_local_target[fk + 1] {
            let a = body[fk];
            let b = body[fk + 1];
            if a.op == Op::AddImm
                && a.imm == 1
                && a.a >= local_count
                && b.op == Op::ArrGetInt
                && b.imm == 0
                && b.b == arr_reg
                && b.c == a.a
                && temp_dead_in_list_body(body, fk + 1, a.a)
            {
                fused[fk] = Fused::GetPlus1;
                fused[fk + 1] = Fused::Consumed;
                fk += 2;
                continue;
            }
            if a.op == Op::ArrGetInt
                && a.imm == 0
                && a.b == arr_reg
                && a.a >= local_count
                && b.op == Op::ArrSetInt
                && b.a == arr_reg
                && b.c == a.a
                && b.b != a.a
                && temp_dead_in_list_body(body, fk + 1, a.a)
            {
                fused[fk] = Fused::Copy;
                fused[fk + 1] = Fused::Consumed;
                fk += 2;
                continue;
            }
        }
        fk += 1;
    }

    // 2. Unify register kinds (`Int` vs `Elem`) across up to 4 fixed-point passes.
    let mut kinds = [LKind::Unknown; 256];
    let mut written = [false; 256];
    let mut read_any = [false; 256];

    let set_k = |kinds: &mut [LKind; 256], r: u8, k: LKind, changed: &mut bool| {
        let u = kinds[r as usize].unify(k);
        if u != kinds[r as usize] {
            kinds[r as usize] = u;
            *changed = true;
        }
    };

    for _ in 0..4 {
        let mut changed = false;
        for (idx, &op) in body.iter().enumerate() {
            match fused[idx] {
                Fused::Consumed => continue,
                Fused::GetPlus1 => {
                    let b = body[idx + 1];
                    if b.a == arr_reg {
                        return None;
                    }
                    set_k(&mut kinds, op.b, LKind::Int, &mut changed);
                    set_k(&mut kinds, b.a, LKind::Elem, &mut changed);
                    read_any[op.b as usize] = true;
                    written[b.a as usize] = true;
                    continue;
                }
                Fused::Copy => {
                    let b = body[idx + 1];
                    set_k(&mut kinds, op.c, LKind::Int, &mut changed);
                    set_k(&mut kinds, b.b, LKind::Int, &mut changed);
                    read_any[op.c as usize] = true;
                    read_any[b.b as usize] = true;
                    continue;
                }
                Fused::None => {}
            }
            match op.op {
                Op::ArrGetInt if op.imm == 0 => {
                    if op.a == arr_reg {
                        return None;
                    }
                    set_k(&mut kinds, op.c, LKind::Int, &mut changed);
                    set_k(&mut kinds, op.a, LKind::Elem, &mut changed);
                    read_any[op.c as usize] = true;
                    written[op.a as usize] = true;
                }
                Op::ArrSetInt => {
                    set_k(&mut kinds, op.b, LKind::Int, &mut changed);
                    set_k(&mut kinds, op.c, LKind::Elem, &mut changed);
                    read_any[op.b as usize] = true;
                    read_any[op.c as usize] = true;
                }
                Op::ArrSetRegStepLoop if (op.imm & (1 << 23)) != 0 => {
                    set_k(&mut kinds, op.b, LKind::Int, &mut changed);
                    set_k(&mut kinds, op.c, LKind::Int, &mut changed);
                    read_any[op.b as usize] = true;
                    read_any[op.c as usize] = true;
                    written[op.b as usize] = true;
                }
                Op::Move | Op::Assign | Op::AssignTake => {
                    if op.a == arr_reg {
                        return None;
                    }
                    let u = kinds[op.a as usize].unify(kinds[op.b as usize]);
                    set_k(&mut kinds, op.a, u, &mut changed);
                    set_k(&mut kinds, op.b, u, &mut changed);
                    read_any[op.b as usize] = true;
                    written[op.a as usize] = true;
                }
                Op::LoadInt => {
                    if op.a == arr_reg {
                        return None;
                    }
                    // Allow LoadInt to serve either Int or Elem (e.g. `$flags[$k] = 0`).
                    written[op.a as usize] = true;
                }
                Op::AddNum | Op::SubNum | Op::MulNum => {
                    if op.a == arr_reg {
                        return None;
                    }
                    set_k(&mut kinds, op.a, LKind::Int, &mut changed);
                    set_k(&mut kinds, op.b, LKind::Int, &mut changed);
                    set_k(&mut kinds, op.c, LKind::Int, &mut changed);
                    read_any[op.b as usize] = true;
                    read_any[op.c as usize] = true;
                    written[op.a as usize] = true;
                }
                Op::AddImm | Op::SubImm | Op::ShlImm | Op::ShrImm => {
                    if op.a == arr_reg {
                        return None;
                    }
                    set_k(&mut kinds, op.a, LKind::Int, &mut changed);
                    set_k(&mut kinds, op.b, LKind::Int, &mut changed);
                    read_any[op.b as usize] = true;
                    written[op.a as usize] = true;
                }
                Op::AddAssign | Op::SubAssign | Op::MulAssign => {
                    if op.a == arr_reg {
                        return None;
                    }
                    set_k(&mut kinds, op.a, LKind::Int, &mut changed);
                    set_k(&mut kinds, op.b, LKind::Int, &mut changed);
                    read_any[op.a as usize] = true;
                    read_any[op.b as usize] = true;
                    written[op.a as usize] = true;
                }
                Op::IncLocalFast | Op::DecLocalFast => {
                    if op.a == arr_reg {
                        return None;
                    }
                    set_k(&mut kinds, op.a, LKind::Int, &mut changed);
                    read_any[op.a as usize] = true;
                    written[op.a as usize] = true;
                }
                Op::IncDecLocal => {
                    if op.a == arr_reg || op.b == arr_reg {
                        return None;
                    }
                    set_k(&mut kinds, op.a, LKind::Int, &mut changed);
                    set_k(&mut kinds, op.b, LKind::Int, &mut changed);
                    read_any[op.b as usize] = true;
                    written[op.a as usize] = true;
                    written[op.b as usize] = true;
                }
                Op::BranchCmp => {
                    let u = kinds[op.b as usize].unify(kinds[op.c as usize]);
                    set_k(&mut kinds, op.b, u, &mut changed);
                    set_k(&mut kinds, op.c, u, &mut changed);
                    read_any[op.b as usize] = true;
                    read_any[op.c as usize] = true;
                }
                Op::BranchLtImm
                | Op::BranchLeImm
                | Op::BranchGtImm
                | Op::BranchGeImm
                | Op::BranchEqImm
                | Op::BranchNeImm
                | Op::BranchIdentImm
                | Op::BranchNotIdentImm => {
                    read_any[op.a as usize] = true;
                }
                Op::IncLoopLtImm | Op::IncLoopLeImm | Op::DecLoopGeImm | Op::DecLoopGtImm => {
                    if op.a == arr_reg {
                        return None;
                    }
                    set_k(&mut kinds, op.a, LKind::Int, &mut changed);
                    read_any[op.a as usize] = true;
                    written[op.a as usize] = true;
                }
                Op::IncLoopLtReg | Op::IncLoopLeReg | Op::DecLoopGeReg | Op::DecLoopGtReg => {
                    if op.a == arr_reg {
                        return None;
                    }
                    set_k(&mut kinds, op.a, LKind::Int, &mut changed);
                    set_k(&mut kinds, op.b, LKind::Int, &mut changed);
                    read_any[op.a as usize] = true;
                    read_any[op.b as usize] = true;
                    written[op.a as usize] = true;
                }
                Op::Jump | Op::ReturnNull | Op::RetIntImm | Op::ReturnReg => {}
                Op::Clear if op.a >= local_count => {}
                _ => return None,
            }
        }
        if !changed {
            break;
        }
    }

    // Default any remaining `Unknown` registers that are read or written to `Int`.
    for r in 0..256 {
        if kinds[r] == LKind::Conflict {
            return None;
        }
        if kinds[r] == LKind::Unknown && (read_any[r] || written[r]) {
            kinds[r] = LKind::Int;
        }
    }
    if kinds[arr_reg as usize] != LKind::Unknown {
        return None;
    }

    // Reject `BranchIdentImm`/`BranchNotIdentImm` on `Elem` (strict `===` int vs float differs!).
    for &op in body {
        if matches!(op.op, Op::BranchIdentImm | Op::BranchNotIdentImm)
            && kinds[op.a as usize] == LKind::Elem
        {
            return None;
        }
    }

    // 3. Exact CFG `live_in` reachability per used register: a register is live-in
    //    iff some execution path from local IP 0 reaches a READ before any WRITE.
    let mut live_in = [false; 256];
    for r in 0..256 {
        if !read_any[r] {
            continue;
        }
        live_in[r] = reg_live_in_body(body, body_start, end, back_target, r as u8);
    }

    // 4. Assign dense variable slots in `iv` (<= 32) and `ev` (<= 16).
    let mut int_regs: Vec<u8> = Vec::new();
    let mut elem_regs: Vec<u8> = Vec::new();
    let mut int_slot = [u8::MAX; 256];
    let mut elem_slot = [u8::MAX; 256];
    let mut live_in_ints: Vec<u8> = Vec::new();
    let mut live_in_elems: Vec<u8> = Vec::new();
    let mut written_ints: Vec<u8> = Vec::new();
    let mut written_elems: Vec<u8> = Vec::new();

    for r in 0..256 {
        match kinds[r] {
            LKind::Int => {
                let s = int_regs.len();
                if s >= MAX_LIST_INT_SLOTS {
                    return None;
                }
                int_regs.push(r as u8);
                int_slot[r] = s as u8;
                if live_in[r] {
                    live_in_ints.push(s as u8);
                }
                if written[r] {
                    written_ints.push(s as u8);
                }
            }
            LKind::Elem => {
                let s = elem_regs.len();
                if s >= MAX_LIST_ELEM_SLOTS {
                    return None;
                }
                elem_regs.push(r as u8);
                elem_slot[r] = s as u8;
                if live_in[r] {
                    live_in_elems.push(s as u8);
                }
                if written[r] {
                    written_elems.push(s as u8);
                }
            }
            _ => {}
        }
    }

    // 5. Lower every `FastOp` in `body` to 1 `ListOp` (1-to-1 IP mapping,
    //    skipping dead `Clear(temp)` and fused second ops via `old_to_local`).
    let mut old_to_local = vec![0usize; n + 1];
    let mut local_count_ops = 0usize;
    for (k, &op) in body.iter().enumerate() {
        old_to_local[k] = local_count_ops;
        if op.op != Op::Clear && fused[k] != Fused::Consumed {
            local_count_ops += 1;
        }
    }
    old_to_local[n] = local_count_ops;

    let map_target = |fast_target: usize| -> Option<(usize, bool)> {
        if fast_target == back_target || fast_target == body_start {
            Some((0, false))
        } else if fast_target > body_start && fast_target <= end {
            Some((old_to_local[fast_target - body_start], false))
        } else if fast_target < 16_384 {
            Some((fast_target, true))
        } else {
            None
        }
    };

    let mut lops = Vec::with_capacity(local_count_ops);
    let mut fast_ips = Vec::with_capacity(local_count_ops);

    for (k, &op) in body.iter().enumerate() {
        if op.op == Op::Clear || fused[k] == Fused::Consumed {
            continue;
        }
        let fip = (body_start + k) as u16;
        if fused[k] == Fused::GetPlus1 {
            let b = body[k + 1];
            lops.push(ListOp::new(
                LOp::ListGetPlus1,
                elem_slot[b.a as usize],
                int_slot[op.b as usize],
                0,
                0,
            ));
            fast_ips.push(fip);
            continue;
        }
        if fused[k] == Fused::Copy {
            let b = body[k + 1];
            lops.push(ListOp::new(
                LOp::ListCopy,
                int_slot[b.b as usize],
                int_slot[op.c as usize],
                0,
                0,
            ));
            fast_ips.push(fip);
            continue;
        }
        let lop = match op.op {
            Op::ArrGetInt => ListOp::new(
                LOp::ListGet,
                elem_slot[op.a as usize],
                int_slot[op.c as usize],
                0,
                0,
            ),
            Op::ArrSetInt => ListOp::new(
                LOp::ListSet,
                int_slot[op.b as usize],
                elem_slot[op.c as usize],
                0,
                0,
            ),
            Op::ArrSetRegStepLoop => ListOp::new(
                LOp::FillStep,
                int_slot[op.b as usize],
                int_slot[op.c as usize],
                0,
                op.imm,
            ),
            Op::Move | Op::Assign | Op::AssignTake => match kinds[op.a as usize] {
                LKind::Elem => ListOp::new(
                    LOp::MovElem,
                    elem_slot[op.a as usize],
                    elem_slot[op.b as usize],
                    0,
                    0,
                ),
                LKind::Int => ListOp::new(
                    LOp::MovI64,
                    int_slot[op.a as usize],
                    int_slot[op.b as usize],
                    0,
                    0,
                ),
                _ => return None,
            },
            Op::LoadInt => match kinds[op.a as usize] {
                LKind::Elem => {
                    ListOp::new(LOp::LoadElemImm, elem_slot[op.a as usize], 0, 0, op.imm)
                }
                LKind::Int => ListOp::new(LOp::LoadI64, int_slot[op.a as usize], 0, 0, op.imm),
                _ => return None,
            },
            Op::AddNum => ListOp::new(
                LOp::AddI64,
                int_slot[op.a as usize],
                int_slot[op.b as usize],
                int_slot[op.c as usize],
                0,
            ),
            Op::SubNum => ListOp::new(
                LOp::SubI64,
                int_slot[op.a as usize],
                int_slot[op.b as usize],
                int_slot[op.c as usize],
                0,
            ),
            Op::MulNum => ListOp::new(
                LOp::MulI64,
                int_slot[op.a as usize],
                int_slot[op.b as usize],
                int_slot[op.c as usize],
                0,
            ),
            Op::AddAssign => ListOp::new(
                LOp::AddI64,
                int_slot[op.a as usize],
                int_slot[op.a as usize],
                int_slot[op.b as usize],
                0,
            ),
            Op::SubAssign => ListOp::new(
                LOp::SubI64,
                int_slot[op.a as usize],
                int_slot[op.a as usize],
                int_slot[op.b as usize],
                0,
            ),
            Op::MulAssign => ListOp::new(
                LOp::MulI64,
                int_slot[op.a as usize],
                int_slot[op.a as usize],
                int_slot[op.b as usize],
                0,
            ),
            Op::AddImm => ListOp::new(
                LOp::AddImmI64,
                int_slot[op.a as usize],
                int_slot[op.b as usize],
                0,
                op.imm,
            ),
            Op::SubImm => ListOp::new(
                LOp::SubImmI64,
                int_slot[op.a as usize],
                int_slot[op.b as usize],
                0,
                op.imm,
            ),
            Op::ShlImm => ListOp::new(
                LOp::ShlImmI64,
                int_slot[op.a as usize],
                int_slot[op.b as usize],
                0,
                op.imm,
            ),
            Op::ShrImm => ListOp::new(
                LOp::ShrImmI64,
                int_slot[op.a as usize],
                int_slot[op.b as usize],
                0,
                op.imm,
            ),
            Op::IncLocalFast => ListOp::new(LOp::IncI64, int_slot[op.a as usize], 0, 0, 0),
            Op::DecLocalFast => ListOp::new(LOp::DecI64, int_slot[op.a as usize], 0, 0, 0),
            Op::IncDecLocal => {
                let lop_code = match op.c {
                    0 => LOp::PreIncI64,
                    1 => LOp::PostIncI64,
                    2 => LOp::PreDecI64,
                    3 => LOp::PostDecI64,
                    _ => return None,
                };
                ListOp::new(
                    lop_code,
                    int_slot[op.a as usize],
                    int_slot[op.b as usize],
                    0,
                    0,
                )
            }
            Op::ReturnNull | Op::RetIntImm | Op::ReturnReg => {
                ListOp::new(LOp::ExitToFast, 0, 0, 0, i32::from(fip))
            }
            Op::Jump => {
                let (t, ext) = map_target(op.imm as usize)?;
                ListOp::new(LOp::Jump, 0, 0, u8::from(ext), t as i32)
            }
            Op::BranchCmp => {
                let (t, ext) = map_target(op.imm as usize)?;
                let flags = op.a | if ext { 0x80 } else { 0 };
                match kinds[op.b as usize] {
                    LKind::Elem => ListOp::new(
                        LOp::CmpElemBranch,
                        flags,
                        elem_slot[op.b as usize],
                        elem_slot[op.c as usize],
                        t as i32,
                    ),
                    LKind::Int => ListOp::new(
                        LOp::CmpI64Branch,
                        flags,
                        int_slot[op.b as usize],
                        int_slot[op.c as usize],
                        t as i32,
                    ),
                    _ => return None,
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
                let cmp = branch_imm_cmp_code(op.op)?;
                let when = (op.c & 0x80) != 0;
                let fast_t = (op.b as usize) | (((op.c & 0x7f) as usize) << 8);
                let (t, ext) = map_target(fast_t)?;
                if t >= 16_384 {
                    return None;
                }
                let c_byte = (((t >> 8) & 0x3f) as u8)
                    | if ext { 0x40 } else { 0 }
                    | if when { 0x80 } else { 0 };
                match kinds[op.a as usize] {
                    LKind::Elem => ListOp::new(
                        LOp::CmpElemImmBranch,
                        (elem_slot[op.a as usize] & 15) | ((cmp & 7) << 4),
                        (t & 0xff) as u8,
                        c_byte,
                        op.imm,
                    ),
                    LKind::Int => ListOp::new(
                        LOp::CmpI64ImmBranch,
                        (int_slot[op.a as usize] & 31) | ((cmp & 7) << 5),
                        (t & 0xff) as u8,
                        c_byte,
                        op.imm,
                    ),
                    _ => return None,
                }
            }
            Op::IncLoopLtImm | Op::IncLoopLeImm | Op::DecLoopGeImm | Op::DecLoopGtImm => {
                let fast_t = (op.b as usize) | ((op.c as usize) << 8);
                let (t, ext) = map_target(fast_t)?;
                if t >= 32_768 {
                    return None;
                }
                let lop_code = match op.op {
                    Op::IncLoopLtImm => LOp::IncLoopLtImm,
                    Op::IncLoopLeImm => LOp::IncLoopLeImm,
                    Op::DecLoopGeImm => LOp::DecLoopGeImm,
                    Op::DecLoopGtImm => LOp::DecLoopGtImm,
                    _ => unreachable!(),
                };
                ListOp::new(
                    lop_code,
                    int_slot[op.a as usize],
                    (t & 0xff) as u8,
                    (((t >> 8) & 0x7f) as u8) | if ext { 0x80 } else { 0 },
                    op.imm,
                )
            }
            Op::IncLoopLtReg | Op::IncLoopLeReg | Op::DecLoopGeReg | Op::DecLoopGtReg => {
                let (t, ext) = map_target(op.imm as usize)?;
                let lop_code = match op.op {
                    Op::IncLoopLtReg => LOp::IncLoopLtReg,
                    Op::IncLoopLeReg => LOp::IncLoopLeReg,
                    Op::DecLoopGeReg => LOp::DecLoopGeReg,
                    Op::DecLoopGtReg => LOp::DecLoopGtReg,
                    _ => unreachable!(),
                };
                ListOp::new(
                    lop_code,
                    int_slot[op.a as usize],
                    int_slot[op.b as usize],
                    u8::from(ext),
                    t as i32,
                )
            }
            _ => return None,
        };
        lops.push(lop);
        fast_ips.push(fip);
    }

    Some(ListLoop {
        ops: lops.into_boxed_slice(),
        fast_ips: fast_ips.into_boxed_slice(),
        fallthrough_fast_ip: (end + 1) as u16,
        arr_reg,
        int_regs: int_regs.into_boxed_slice(),
        elem_regs: elem_regs.into_boxed_slice(),
        live_in_ints: live_in_ints.into_boxed_slice(),
        live_in_elems: live_in_elems.into_boxed_slice(),
        written_ints: written_ints.into_boxed_slice(),
        written_elems: written_elems.into_boxed_slice(),
    })
}

fn reg_live_in_body(
    body: &[FastOp],
    body_start: usize,
    end: usize,
    back_target: usize,
    reg: u8,
) -> bool {
    let n = body.len();
    let mut visited = [false; 130];
    let mut stack: Vec<usize> = Vec::with_capacity(16);
    stack.push(0);
    while let Some(mut k) = stack.pop() {
        while k < n {
            if visited[k] {
                break;
            }
            visited[k] = true;
            let op = body[k];
            if fast_op_reads_reg(op, reg) {
                return true;
            }
            if fast_op_writes_reg(op, reg) {
                break;
            }
            if matches!(op.op, Op::ReturnNull | Op::RetIntImm | Op::ReturnReg) {
                break;
            }
            if op.op == Op::Jump {
                let t = op.imm as usize;
                if t == back_target || t == body_start {
                    k = 0;
                    continue;
                } else if t > body_start && t <= end {
                    k = t - body_start;
                    continue;
                } else {
                    break;
                }
            }
            if let Some(t) = fast_op_branch_target(op) {
                if t == back_target || t == body_start {
                    stack.push(0);
                } else if t > body_start && t <= end {
                    stack.push(t - body_start);
                }
            }
            k += 1;
        }
    }
    false
}

pub(crate) fn fast_op_reads_reg_pub(op: FastOp, reg: u8) -> bool {
    fast_op_reads_reg(op, reg)
}

fn temp_dead_in_list_body(body: &[FastOp], after_k: usize, reg: u8) -> bool {
    for &op in &body[(after_k + 1)..] {
        if op.op == Op::Clear && op.a == reg {
            return true;
        }
        if fast_op_reads_reg(op, reg) {
            return false;
        }
        if fast_op_writes_reg(op, reg) {
            return true;
        }
        if fast_op_branch_target(op).is_some()
            || matches!(op.op, Op::ReturnNull | Op::RetIntImm | Op::ReturnReg)
        {
            return !body
                .iter()
                .skip(after_k + 1)
                .any(|&o| fast_op_reads_reg(o, reg));
        }
    }
    true
}

fn fast_op_reads_reg(op: FastOp, reg: u8) -> bool {
    match op.op {
        Op::ArrGetInt => op.c == reg,
        Op::ArrSetInt => op.b == reg || op.c == reg,
        Op::ArrSetRegStepLoop => op.b == reg || op.c == reg,
        Op::Move
        | Op::Assign
        | Op::AssignTake
        | Op::AddImm
        | Op::SubImm
        | Op::ShlImm
        | Op::ShrImm
        | Op::IncDecLocal => op.b == reg,
        Op::AddNum | Op::SubNum | Op::MulNum | Op::BranchCmp => op.b == reg || op.c == reg,
        Op::AddAssign
        | Op::SubAssign
        | Op::MulAssign
        | Op::IncLoopLtReg
        | Op::IncLoopLeReg
        | Op::DecLoopGeReg
        | Op::DecLoopGtReg => op.a == reg || op.b == reg,
        Op::IncLocalFast
        | Op::DecLocalFast
        | Op::IncLoopLtImm
        | Op::IncLoopLeImm
        | Op::DecLoopGeImm
        | Op::DecLoopGtImm
        | Op::BranchLtImm
        | Op::BranchLeImm
        | Op::BranchGtImm
        | Op::BranchGeImm
        | Op::BranchEqImm
        | Op::BranchNeImm
        | Op::BranchIdentImm
        | Op::BranchNotIdentImm
        | Op::ReturnReg => op.a == reg,
        _ => false,
    }
}

fn fast_op_writes_reg(op: FastOp, reg: u8) -> bool {
    match op.op {
        Op::ArrGetInt
        | Op::Move
        | Op::Assign
        | Op::AssignTake
        | Op::LoadInt
        | Op::AddNum
        | Op::SubNum
        | Op::MulNum
        | Op::AddImm
        | Op::SubImm
        | Op::ShlImm
        | Op::ShrImm
        | Op::AddAssign
        | Op::SubAssign
        | Op::MulAssign
        | Op::IncLocalFast
        | Op::DecLocalFast
        | Op::IncLoopLtImm
        | Op::IncLoopLeImm
        | Op::DecLoopGeImm
        | Op::DecLoopGtImm
        | Op::IncLoopLtReg
        | Op::IncLoopLeReg
        | Op::DecLoopGeReg
        | Op::DecLoopGtReg => op.a == reg,
        Op::IncDecLocal => op.a == reg || op.b == reg,
        Op::ArrSetRegStepLoop => op.b == reg,
        _ => false,
    }
}
