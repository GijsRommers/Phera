//! Unboxed tagless inner-loop compiler & leaf executor (100% safe Rust).
//!
//! For any innermost pure-scalar `FastOp` loop (`mandel`, `mandel2`, `simple`,
//! `nestedloop`, numeric accumulations, etc.), extracts a compact [`ScalarLoop`]
//! operating directly on stack-local `[i64; 32]` and `[f64; 32]` arrays:
//! - Zero tag loads/stores or `TAG_HEAP` checks inside the loop
//! - Hoisted float/int constants loaded once at loop entry
//! - General 3-operand arithmetic fusions (`MulAddF64`, `SubAddF64`, `AddAddF64`,
//!   `Mul2MulAddF64`, `AddCmpF64Exit`) and 1-op counter+accumulator loops
//! - Exact overflow, division-by-zero, and yield fallback back to `FastVm`

use crate::fast_tier::{
    CMP_EQ, CMP_GE, CMP_GT, CMP_IDENT, CMP_LE, CMP_LT, CMP_NE, CMP_NOT_IDENT, FastOp, Op,
    TAG_FLOAT, TAG_HEAP, TAG_INT,
};

pub(crate) const MAX_SCALAR_SLOTS: usize = 32;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SOp {
    // 1-op fused counter + accumulator inner loops (`simple`, `nestedloop`, `simpleucall`)
    IncIncLoopLtImm,
    IncIncLoopLeImm,
    IncIncLoopLtReg,
    IncIncLoopLeReg,
    IncOnlyLoopLtImm,
    IncOnlyLoopLeImm,

    // Unboxed float arithmetic & general 3-op fusions
    AddF64,
    SubF64,
    MulF64,
    DivF64,
    NegF64,
    MovF64,
    MulAddF64,     // fv[a] = fv[b] * fv[c] + fv[imm]
    SubAddF64,     // fv[a] = fv[b] - fv[c] + fv[imm]
    AddAddF64,     // fv[a] = fv[b] + fv[c] + fv[imm]
    Mul2MulAddF64, // fv[a] = (fv[b] + fv[b]) * fv[c] + fv[imm]

    // Unboxed int arithmetic
    AddI64,
    SubI64,
    MulI64,
    AddImmI64,
    SubImmI64,
    MulImmI64,
    NegI64,
    MovI64,
    IncI64,
    DecI64,

    // Mixed int/float conversions
    I64ToF64,

    // Loop control & exits
    JumpBack,      // jump to `imm` (back-edge: spends 1 budget)
    AddCmpF64Exit, // if cmp(fv[a] + fv[b], fv[c]) { exit imm } (cmp in low 4 bits of imm>>16)
    CmpF64Exit,    // if cmp(fv[a], fv[b]) { exit imm }
    CmpI64ImmExit, // if cmp(iv[a], imm) { exit target in (b | c<<8) }
    CmpI64Exit,    // if cmp(iv[a], iv[b]) { exit imm }
    CmpI64ImmBack, // if cmp(iv[a], imm) == when { goto 0 (budget) } else { exit fallthrough }
    CmpI64Back,    // if cmp(iv[a], iv[b]) == when { goto 0 (budget) } else { exit fallthrough }
    CmpF64Back,    // if cmp(fv[a], fv[b]) == when { goto 0 (budget) } else { exit fallthrough }
    IncLoopLtImm,  // iv[a] += 1; if iv[a] < imm { goto 0 (budget) } else { exit b|c<<8 }
    IncLoopLeImm,
    DecLoopGeImm,
    DecLoopGtImm,
    IncLoopLtReg, // iv[a] += 1; if iv[a] < iv[b] { goto 0 (budget) } else { exit imm }
    IncLoopLeReg,
    DecLoopGeReg,
    DecLoopGtReg,
}

/// 8-byte unboxed scalar op.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScalarOp {
    pub(crate) op: SOp,
    pub(crate) a: u8,
    pub(crate) b: u8,
    pub(crate) c: u8,
    pub(crate) imm: i32,
}

const _: () = assert!(std::mem::size_of::<ScalarOp>() == 8);

impl ScalarOp {
    #[inline(always)]
    const fn new(op: SOp, a: u8, b: u8, c: u8, imm: i32) -> Self {
        Self { op, a, b, c, imm }
    }
}

/// Metadata + compiled unboxed stream for one innermost scalar loop.
pub(crate) struct ScalarLoop {
    pub(crate) ops: Box<[ScalarOp]>,
    /// Corresponding `FastOp` IP for each `ScalarOp` (for exact overflow/div0 fallback).
    pub(crate) fast_ips: Box<[u16]>,
    /// `FastOp` IP immediately after the loop latch (normal fallthrough exit).
    pub(crate) fallthrough_fast_ip: u16,
    /// FastVm register index for each `iv[0..int_regs.len()]` variable slot.
    pub(crate) int_regs: Box<[u8]>,
    /// FastVm register index for each `fv[0..f64_regs.len()]` variable slot.
    pub(crate) f64_regs: Box<[u8]>,
    /// Slots in `int_regs` that must hold `TAG_INT` on entry.
    pub(crate) live_in_ints: Box<[u8]>,
    /// Slots in `f64_regs` that must hold `TAG_FLOAT` on entry.
    pub(crate) live_in_f64s: Box<[u8]>,
    /// Slots in `int_regs` written by the loop (checked `!= TAG_HEAP` on entry,
    /// written back as `TAG_INT` on exit).
    pub(crate) written_ints: Box<[u8]>,
    /// Slots in `f64_regs` written by the loop (checked `!= TAG_HEAP` on entry,
    /// written back as `TAG_FLOAT` on exit).
    pub(crate) written_f64s: Box<[u8]>,
    /// Hoisted read-only `f64` constants loaded into `fv[f64_regs.len()..]`.
    pub(crate) f64_consts: Box<[f64]>,
    /// Hoisted read-only `i64` constants loaded into `iv[int_regs.len()..]`.
    pub(crate) i64_consts: Box<[i64]>,
}

/// Outcome of running `run_scalar_loop`.
pub(crate) enum ScalarExit {
    /// Exited normally to `next_fast_ip` (all written registers are exact).
    Exit { next_fast_ip: usize, spent: usize },
    /// Work budget exhausted at loop header (write back & yield cleanly).
    Yield,
    /// Arithmetic overflow or division by zero BEFORE mutating `fallback_fast_ip`.
    Fallback {
        fallback_fast_ip: usize,
        spent: usize,
    },
}

/// Entry guard + setup + execution + write-back for `Op::EnterScalarLoop`.
#[inline(always)]
pub(crate) fn try_run_scalar_loop(
    sl: &ScalarLoop,
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    budget: usize,
) -> Option<ScalarExit> {
    // 1. Entry guard: verify every live-in register has its exact expected tag,
    //    and every written register is non-heap (never clobbers a PHP reference!).
    for &slot in sl.live_in_ints.iter() {
        let reg = sl.int_regs[slot as usize] as usize;
        if t[reg] != TAG_INT {
            return None;
        }
    }
    for &slot in sl.live_in_f64s.iter() {
        let reg = sl.f64_regs[slot as usize] as usize;
        if t[reg] != TAG_FLOAT {
            return None;
        }
    }
    for &slot in sl.written_ints.iter() {
        let reg = sl.int_regs[slot as usize] as usize;
        if t[reg] == TAG_HEAP {
            return None;
        }
    }
    for &slot in sl.written_f64s.iter() {
        let reg = sl.f64_regs[slot as usize] as usize;
        if t[reg] == TAG_HEAP {
            return None;
        }
    }

    // 2. Load stack-local unboxed register arrays + hoisted constants.
    let mut iv = [0i64; MAX_SCALAR_SLOTS];
    let mut fv = [0.0f64; MAX_SCALAR_SLOTS];
    for &slot in sl.live_in_ints.iter() {
        let s = slot as usize;
        iv[s] = v[sl.int_regs[s] as usize] as i64;
    }
    let i_const_base = sl.int_regs.len();
    for (k, &c) in sl.i64_consts.iter().enumerate() {
        iv[i_const_base + k] = c;
    }
    for &slot in sl.live_in_f64s.iter() {
        let s = slot as usize;
        fv[s] = f64::from_bits(v[sl.f64_regs[s] as usize]);
    }
    let f_const_base = sl.f64_regs.len();
    for (k, &c) in sl.f64_consts.iter().enumerate() {
        fv[f_const_base + k] = c;
    }

    // 3. Execute the tiny leaf unboxed kernel!
    let exit = run_scalar_kernel(&sl.ops, &sl.fast_ips, &mut iv, &mut fv, budget);

    // 4. Write back all modified registers to `(t, v)`.
    for &slot in sl.written_ints.iter() {
        let s = slot as usize;
        let reg = sl.int_regs[s] as usize;
        t[reg] = TAG_INT;
        v[reg] = iv[s] as u64;
    }
    for &slot in sl.written_f64s.iter() {
        let s = slot as usize;
        let reg = sl.f64_regs[s] as usize;
        t[reg] = TAG_FLOAT;
        v[reg] = fv[s].to_bits();
    }

    Some(exit)
}

#[inline(always)]
fn eval_f64_cmp(cmp: u8, a: f64, b: f64) -> bool {
    match cmp {
        CMP_EQ | CMP_IDENT => a == b,
        CMP_NE | CMP_NOT_IDENT => a != b,
        CMP_LT => a < b,
        CMP_LE => a <= b,
        CMP_GT => a > b,
        CMP_GE => a >= b,
        _ => false,
    }
}

#[inline(always)]
fn eval_i64_cmp(cmp: u8, a: i64, b: i64) -> bool {
    match cmp {
        CMP_EQ | CMP_IDENT => a == b,
        CMP_NE | CMP_NOT_IDENT => a != b,
        CMP_LT => a < b,
        CMP_LE => a <= b,
        CMP_GT => a > b,
        CMP_GE => a >= b,
        _ => false,
    }
}

/// Tiny leaf unboxed inner-loop executor.
/// Zero heap, zero tags, zero `CallStack`, zero deopt code inside!
#[inline(never)]
fn run_scalar_kernel(
    ops: &[ScalarOp],
    fast_ips: &[u16],
    iv: &mut [i64; MAX_SCALAR_SLOTS],
    fv: &mut [f64; MAX_SCALAR_SLOTS],
    budget_start: usize,
) -> ScalarExit {
    let mut ip: usize = 0;
    let mut budget = budget_start.max(1);

    loop {
        let Some(&op) = ops.get(ip) else {
            return ScalarExit::Fallback {
                fallback_fast_ip: fast_ips.first().copied().unwrap_or(0) as usize,
                spent: budget_start.saturating_sub(budget),
            };
        };
        match op.op {
            // =================================================================
            // 1-OP FUSED COUNTER+ACCUMULATOR INNER LOOPS (`simple`, `nestedloop`)
            // =================================================================
            SOp::IncIncLoopLtImm => {
                let a_idx = (op.a & 31) as usize;
                let i_idx = (op.b & 31) as usize;
                let lim = i64::from(op.imm);
                let mut a = iv[a_idx];
                let mut i = iv[i_idx];
                loop {
                    let Some(na) = a.checked_add(1) else {
                        iv[a_idx] = a;
                        iv[i_idx] = i;
                        return ScalarExit::Fallback {
                            fallback_fast_ip: fast_ips[ip] as usize,
                            spent: budget_start.saturating_sub(budget),
                        };
                    };
                    let Some(ni) = i.checked_add(1) else {
                        iv[a_idx] = na;
                        iv[i_idx] = i;
                        return ScalarExit::Fallback {
                            fallback_fast_ip: fast_ips[ip] as usize,
                            spent: budget_start.saturating_sub(budget),
                        };
                    };
                    a = na;
                    i = ni;
                    if i >= lim {
                        iv[a_idx] = a;
                        iv[i_idx] = i;
                        return ScalarExit::Exit {
                            next_fast_ip: usize::MAX, // caller uses fallthrough_fast_ip
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                    budget -= 1;
                    if budget == 0 {
                        iv[a_idx] = a;
                        iv[i_idx] = i;
                        return ScalarExit::Yield;
                    }
                }
            }
            SOp::IncIncLoopLeImm => {
                let a_idx = (op.a & 31) as usize;
                let i_idx = (op.b & 31) as usize;
                let lim = i64::from(op.imm);
                let mut a = iv[a_idx];
                let mut i = iv[i_idx];
                loop {
                    let Some(na) = a.checked_add(1) else {
                        iv[a_idx] = a;
                        iv[i_idx] = i;
                        return ScalarExit::Fallback {
                            fallback_fast_ip: fast_ips[ip] as usize,
                            spent: budget_start.saturating_sub(budget),
                        };
                    };
                    let Some(ni) = i.checked_add(1) else {
                        iv[a_idx] = na;
                        iv[i_idx] = i;
                        return ScalarExit::Fallback {
                            fallback_fast_ip: fast_ips[ip] as usize,
                            spent: budget_start.saturating_sub(budget),
                        };
                    };
                    a = na;
                    i = ni;
                    if i > lim {
                        iv[a_idx] = a;
                        iv[i_idx] = i;
                        return ScalarExit::Exit {
                            next_fast_ip: usize::MAX,
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                    budget -= 1;
                    if budget == 0 {
                        iv[a_idx] = a;
                        iv[i_idx] = i;
                        return ScalarExit::Yield;
                    }
                }
            }
            SOp::IncIncLoopLtReg => {
                let a_idx = (op.a & 31) as usize;
                let i_idx = (op.b & 31) as usize;
                let lim = iv[(op.c & 31) as usize];
                let mut a = iv[a_idx];
                let mut i = iv[i_idx];
                loop {
                    let Some(na) = a.checked_add(1) else {
                        iv[a_idx] = a;
                        iv[i_idx] = i;
                        return ScalarExit::Fallback {
                            fallback_fast_ip: fast_ips[ip] as usize,
                            spent: budget_start.saturating_sub(budget),
                        };
                    };
                    let Some(ni) = i.checked_add(1) else {
                        iv[a_idx] = na;
                        iv[i_idx] = i;
                        return ScalarExit::Fallback {
                            fallback_fast_ip: fast_ips[ip] as usize,
                            spent: budget_start.saturating_sub(budget),
                        };
                    };
                    a = na;
                    i = ni;
                    if i >= lim {
                        iv[a_idx] = a;
                        iv[i_idx] = i;
                        return ScalarExit::Exit {
                            next_fast_ip: usize::MAX,
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                    budget -= 1;
                    if budget == 0 {
                        iv[a_idx] = a;
                        iv[i_idx] = i;
                        return ScalarExit::Yield;
                    }
                }
            }
            SOp::IncIncLoopLeReg => {
                let a_idx = (op.a & 31) as usize;
                let i_idx = (op.b & 31) as usize;
                let lim = iv[(op.c & 31) as usize];
                let mut a = iv[a_idx];
                let mut i = iv[i_idx];
                loop {
                    let Some(na) = a.checked_add(1) else {
                        iv[a_idx] = a;
                        iv[i_idx] = i;
                        return ScalarExit::Fallback {
                            fallback_fast_ip: fast_ips[ip] as usize,
                            spent: budget_start.saturating_sub(budget),
                        };
                    };
                    let Some(ni) = i.checked_add(1) else {
                        iv[a_idx] = na;
                        iv[i_idx] = i;
                        return ScalarExit::Fallback {
                            fallback_fast_ip: fast_ips[ip] as usize,
                            spent: budget_start.saturating_sub(budget),
                        };
                    };
                    a = na;
                    i = ni;
                    if i > lim {
                        iv[a_idx] = a;
                        iv[i_idx] = i;
                        return ScalarExit::Exit {
                            next_fast_ip: usize::MAX,
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                    budget -= 1;
                    if budget == 0 {
                        iv[a_idx] = a;
                        iv[i_idx] = i;
                        return ScalarExit::Yield;
                    }
                }
            }
            SOp::IncOnlyLoopLtImm => {
                let i_idx = (op.a & 31) as usize;
                let lim = i64::from(op.imm);
                let mut i = iv[i_idx];
                loop {
                    let Some(ni) = i.checked_add(1) else {
                        iv[i_idx] = i;
                        return ScalarExit::Fallback {
                            fallback_fast_ip: fast_ips[ip] as usize,
                            spent: budget_start.saturating_sub(budget),
                        };
                    };
                    i = ni;
                    if i >= lim {
                        iv[i_idx] = i;
                        return ScalarExit::Exit {
                            next_fast_ip: usize::MAX,
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                    budget -= 1;
                    if budget == 0 {
                        iv[i_idx] = i;
                        return ScalarExit::Yield;
                    }
                }
            }
            SOp::IncOnlyLoopLeImm => {
                let i_idx = (op.a & 31) as usize;
                let lim = i64::from(op.imm);
                let mut i = iv[i_idx];
                loop {
                    let Some(ni) = i.checked_add(1) else {
                        iv[i_idx] = i;
                        return ScalarExit::Fallback {
                            fallback_fast_ip: fast_ips[ip] as usize,
                            spent: budget_start.saturating_sub(budget),
                        };
                    };
                    i = ni;
                    if i > lim {
                        iv[i_idx] = i;
                        return ScalarExit::Exit {
                            next_fast_ip: usize::MAX,
                            spent: budget_start.saturating_sub(budget),
                        };
                    }
                    budget -= 1;
                    if budget == 0 {
                        iv[i_idx] = i;
                        return ScalarExit::Yield;
                    }
                }
            }

            // =================================================================
            // UNBOXED FLOAT ARITHMETIC & GENERAL 3-OP FUSIONS
            // =================================================================
            SOp::AddF64 => {
                fv[(op.a & 31) as usize] = fv[(op.b & 31) as usize] + fv[(op.c & 31) as usize];
                ip += 1;
            }
            SOp::SubF64 => {
                fv[(op.a & 31) as usize] = fv[(op.b & 31) as usize] - fv[(op.c & 31) as usize];
                ip += 1;
            }
            SOp::MulF64 => {
                fv[(op.a & 31) as usize] = fv[(op.b & 31) as usize] * fv[(op.c & 31) as usize];
                ip += 1;
            }
            SOp::DivF64 => {
                let rhs = fv[(op.c & 31) as usize];
                if rhs == 0.0 {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
                fv[(op.a & 31) as usize] = fv[(op.b & 31) as usize] / rhs;
                ip += 1;
            }
            SOp::NegF64 => {
                fv[(op.a & 31) as usize] = -fv[(op.b & 31) as usize];
                ip += 1;
            }
            SOp::MovF64 => {
                fv[(op.a & 31) as usize] = fv[(op.b & 31) as usize];
                ip += 1;
            }
            SOp::MulAddF64 => {
                fv[(op.a & 31) as usize] = fv[(op.b & 31) as usize] * fv[(op.c & 31) as usize]
                    + fv[(op.imm & 31) as usize];
                ip += 1;
            }
            SOp::SubAddF64 => {
                fv[(op.a & 31) as usize] = fv[(op.b & 31) as usize] - fv[(op.c & 31) as usize]
                    + fv[(op.imm & 31) as usize];
                ip += 1;
            }
            SOp::AddAddF64 => {
                fv[(op.a & 31) as usize] = fv[(op.b & 31) as usize]
                    + fv[(op.c & 31) as usize]
                    + fv[(op.imm & 31) as usize];
                ip += 1;
            }
            SOp::Mul2MulAddF64 => {
                let x = fv[(op.b & 31) as usize];
                fv[(op.a & 31) as usize] =
                    (x + x) * fv[(op.c & 31) as usize] + fv[(op.imm & 31) as usize];
                ip += 1;
            }

            // =================================================================
            // UNBOXED INT ARITHMETIC
            // =================================================================
            SOp::AddI64 => {
                let Some(s) = iv[(op.b & 31) as usize].checked_add(iv[(op.c & 31) as usize]) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[(op.a & 31) as usize] = s;
                ip += 1;
            }
            SOp::SubI64 => {
                let Some(s) = iv[(op.b & 31) as usize].checked_sub(iv[(op.c & 31) as usize]) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[(op.a & 31) as usize] = s;
                ip += 1;
            }
            SOp::MulI64 => {
                let Some(s) = iv[(op.b & 31) as usize].checked_mul(iv[(op.c & 31) as usize]) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[(op.a & 31) as usize] = s;
                ip += 1;
            }
            SOp::AddImmI64 => {
                let Some(s) = iv[(op.b & 31) as usize].checked_add(i64::from(op.imm)) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[(op.a & 31) as usize] = s;
                ip += 1;
            }
            SOp::SubImmI64 => {
                let Some(s) = iv[(op.b & 31) as usize].checked_sub(i64::from(op.imm)) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[(op.a & 31) as usize] = s;
                ip += 1;
            }
            SOp::MulImmI64 => {
                let Some(s) = iv[(op.b & 31) as usize].checked_mul(i64::from(op.imm)) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[(op.a & 31) as usize] = s;
                ip += 1;
            }
            SOp::NegI64 => {
                let Some(s) = iv[(op.b & 31) as usize].checked_neg() else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[(op.a & 31) as usize] = s;
                ip += 1;
            }
            SOp::MovI64 => {
                iv[(op.a & 31) as usize] = iv[(op.b & 31) as usize];
                ip += 1;
            }
            SOp::IncI64 => {
                let idx = (op.a & 31) as usize;
                let Some(s) = iv[idx].checked_add(1) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[idx] = s;
                ip += 1;
            }
            SOp::DecI64 => {
                let idx = (op.a & 31) as usize;
                let Some(s) = iv[idx].checked_sub(1) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[idx] = s;
                ip += 1;
            }
            SOp::I64ToF64 => {
                fv[(op.a & 31) as usize] = iv[(op.b & 31) as usize] as f64;
                ip += 1;
            }

            // =================================================================
            // CONTROL FLOW & LOOP LATCHES
            // =================================================================
            SOp::JumpBack => {
                budget -= 1;
                if budget == 0 {
                    return ScalarExit::Yield;
                }
                ip = op.imm as usize;
            }
            SOp::AddCmpF64Exit => {
                let sum = fv[(op.a & 31) as usize] + fv[(op.b & 31) as usize];
                let rhs = fv[(op.c & 31) as usize];
                let flags = ((op.imm as u32) >> 16) as u8;
                let cmp = flags & 0x0f;
                let when = (flags & 0x10) != 0;
                if eval_f64_cmp(cmp, sum, rhs) == when {
                    return ScalarExit::Exit {
                        next_fast_ip: (op.imm & 0xffff) as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
                ip += 1;
            }
            SOp::CmpF64Exit => {
                let flags = op.c;
                let cmp = flags & 0x0f;
                let when = (flags & 0x10) != 0;
                if eval_f64_cmp(cmp, fv[(op.a & 31) as usize], fv[(op.b & 31) as usize]) == when {
                    return ScalarExit::Exit {
                        next_fast_ip: op.imm as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
                ip += 1;
            }
            SOp::CmpI64ImmExit => {
                let reg = (op.a & 31) as usize;
                let cmp = (op.a >> 5) & 7;
                let when = (op.c & 0x80) != 0;
                let target = (op.b as usize) | (((op.c & 0x7f) as usize) << 8);
                if eval_i64_cmp(cmp, iv[reg], i64::from(op.imm)) == when {
                    return ScalarExit::Exit {
                        next_fast_ip: target,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
                ip += 1;
            }
            SOp::CmpI64Exit => {
                let flags = op.c;
                let cmp = flags & 0x0f;
                let when = (flags & 0x10) != 0;
                if eval_i64_cmp(cmp, iv[(op.a & 31) as usize], iv[(op.b & 31) as usize]) == when {
                    return ScalarExit::Exit {
                        next_fast_ip: op.imm as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
                ip += 1;
            }
            SOp::CmpI64ImmBack => {
                let reg = (op.a & 31) as usize;
                let cmp = (op.a >> 5) & 7;
                let when = (op.c & 0x80) != 0;
                if eval_i64_cmp(cmp, iv[reg], i64::from(op.imm)) == when {
                    budget -= 1;
                    if budget == 0 {
                        return ScalarExit::Yield;
                    }
                    ip = 0;
                } else {
                    return ScalarExit::Exit {
                        next_fast_ip: usize::MAX,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
            }
            SOp::CmpI64Back => {
                let flags = op.c;
                let cmp = flags & 0x0f;
                let when = (flags & 0x10) != 0;
                if eval_i64_cmp(cmp, iv[(op.a & 31) as usize], iv[(op.b & 31) as usize]) == when {
                    budget -= 1;
                    if budget == 0 {
                        return ScalarExit::Yield;
                    }
                    ip = 0;
                } else {
                    return ScalarExit::Exit {
                        next_fast_ip: usize::MAX,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
            }
            SOp::CmpF64Back => {
                let flags = op.c;
                let cmp = flags & 0x0f;
                let when = (flags & 0x10) != 0;
                if eval_f64_cmp(cmp, fv[(op.a & 31) as usize], fv[(op.b & 31) as usize]) == when {
                    budget -= 1;
                    if budget == 0 {
                        return ScalarExit::Yield;
                    }
                    ip = 0;
                } else {
                    return ScalarExit::Exit {
                        next_fast_ip: usize::MAX,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
            }
            SOp::IncLoopLtImm => {
                let idx = (op.a & 31) as usize;
                let Some(next) = iv[idx].checked_add(1) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[idx] = next;
                if next < i64::from(op.imm) {
                    budget -= 1;
                    if budget == 0 {
                        return ScalarExit::Yield;
                    }
                    ip = 0;
                } else {
                    return ScalarExit::Exit {
                        next_fast_ip: usize::MAX,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
            }
            SOp::IncLoopLeImm => {
                let idx = (op.a & 31) as usize;
                let Some(next) = iv[idx].checked_add(1) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[idx] = next;
                if next <= i64::from(op.imm) {
                    budget -= 1;
                    if budget == 0 {
                        return ScalarExit::Yield;
                    }
                    ip = 0;
                } else {
                    return ScalarExit::Exit {
                        next_fast_ip: usize::MAX,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
            }
            SOp::DecLoopGeImm => {
                let idx = (op.a & 31) as usize;
                let Some(next) = iv[idx].checked_sub(1) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[idx] = next;
                if next >= i64::from(op.imm) {
                    budget -= 1;
                    if budget == 0 {
                        return ScalarExit::Yield;
                    }
                    ip = 0;
                } else {
                    return ScalarExit::Exit {
                        next_fast_ip: usize::MAX,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
            }
            SOp::DecLoopGtImm => {
                let idx = (op.a & 31) as usize;
                let Some(next) = iv[idx].checked_sub(1) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[idx] = next;
                if next > i64::from(op.imm) {
                    budget -= 1;
                    if budget == 0 {
                        return ScalarExit::Yield;
                    }
                    ip = 0;
                } else {
                    return ScalarExit::Exit {
                        next_fast_ip: usize::MAX,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
            }
            SOp::IncLoopLtReg => {
                let idx = (op.a & 31) as usize;
                let Some(next) = iv[idx].checked_add(1) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[idx] = next;
                if next < iv[(op.b & 31) as usize] {
                    budget -= 1;
                    if budget == 0 {
                        return ScalarExit::Yield;
                    }
                    ip = 0;
                } else {
                    return ScalarExit::Exit {
                        next_fast_ip: usize::MAX,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
            }
            SOp::IncLoopLeReg => {
                let idx = (op.a & 31) as usize;
                let Some(next) = iv[idx].checked_add(1) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[idx] = next;
                if next <= iv[(op.b & 31) as usize] {
                    budget -= 1;
                    if budget == 0 {
                        return ScalarExit::Yield;
                    }
                    ip = 0;
                } else {
                    return ScalarExit::Exit {
                        next_fast_ip: usize::MAX,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
            }
            SOp::DecLoopGeReg => {
                let idx = (op.a & 31) as usize;
                let Some(next) = iv[idx].checked_sub(1) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[idx] = next;
                if next >= iv[(op.b & 31) as usize] {
                    budget -= 1;
                    if budget == 0 {
                        return ScalarExit::Yield;
                    }
                    ip = 0;
                } else {
                    return ScalarExit::Exit {
                        next_fast_ip: usize::MAX,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
            }
            SOp::DecLoopGtReg => {
                let idx = (op.a & 31) as usize;
                let Some(next) = iv[idx].checked_sub(1) else {
                    return ScalarExit::Fallback {
                        fallback_fast_ip: fast_ips[ip] as usize,
                        spent: budget_start.saturating_sub(budget),
                    };
                };
                iv[idx] = next;
                if next > iv[(op.b & 31) as usize] {
                    budget -= 1;
                    if budget == 0 {
                        return ScalarExit::Yield;
                    }
                    ip = 0;
                } else {
                    return ScalarExit::Exit {
                        next_fast_ip: usize::MAX,
                        spent: budget_start.saturating_sub(budget),
                    };
                }
            }
        }
    }
}

// =============================================================================
// Compiler: extract `ScalarLoop`s from lowered `FastOp` streams
// =============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum RegKind {
    Unknown,
    Int,
    Float,
    Conflict,
}

impl RegKind {
    fn unify(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unknown, x) | (x, Self::Unknown) => x,
            (a, b) if a == b => a,
            _ => Self::Conflict,
        }
    }
}

/// Recognize an inline early-exit block inside a loop:
/// `Branch(when, skip_to)` followed by `ReturnReg`/`RetIntImm`/`ReturnNull`
/// OR `Jump(exit > end)` (a `break;`), optionally followed by a dead
/// `Jump(skip_to)`. Returns `(next_off, exit_fast_ip)`.
fn inline_early_exit(
    body: &[FastOp],
    body_start: usize,
    end: usize,
    off: usize,
) -> Option<(usize, usize)> {
    let op = body[off];
    let skip_target = fast_op_branch_target(op)?;
    if skip_target <= body_start + off {
        return None;
    }
    let skip_off = skip_target - body_start;
    if skip_off > body.len() {
        return None;
    }
    let next_op = *body.get(off + 1)?;
    let exit_ip = match next_op.op {
        Op::ReturnReg | Op::RetIntImm | Op::ReturnNull => body_start + off + 1,
        Op::Jump if (next_op.imm as usize) > end => next_op.imm as usize,
        _ => return None,
    };
    if skip_off == off + 2
        || (skip_off == off + 3 && body.get(off + 2).is_some_and(|j| j.op == Op::Jump))
    {
        Some((skip_off, exit_ip))
    } else {
        None
    }
}

/// Try to compile an innermost pure-scalar loop `ops[body_start..=end]` (whose
/// `EnterScalarLoop` header is at `header_ip`, with `ops[end]` branching back to
/// `back_target`, using FINAL `FastOp` indices!).
#[allow(clippy::too_many_arguments)]
pub(crate) fn try_compile_scalar_loop(
    ops: &[FastOp],
    _header_ip: usize,
    body_start: usize,
    end: usize,
    back_target: usize,
    local_count: u8,
    f64_pool: &[f64],
    float_hints: &[bool],
) -> Option<ScalarLoop> {
    if body_start > end || end >= ops.len() || end - body_start > 64 {
        return None;
    }

    let body = &ops[body_start..=end];
    // Validate control flow: every non-final branch must either exit `> end`
    // OR be an inline early-return (`if (cond) return X;`), and the final op
    // must branch back to `back_target`.
    let mut skip_mask = [false; 65];
    let mut off = 0usize;
    while off < body.len() {
        let is_last = off + 1 == body.len();
        let op = body[off];
        if !is_last {
            if let Some((skip_off, _)) = inline_early_exit(body, body_start, end, off) {
                skip_mask[(off + 1)..skip_off].fill(true);
                off = skip_off;
                continue;
            }
            if let Some(target) = fast_op_branch_target(op)
                && target <= end
            {
                return None;
            }
        } else {
            let target = fast_op_branch_target(op)?;
            if target != back_target {
                return None;
            }
        }
        off += 1;
    }

    // 1. Infer unique Int / Float kind per touched register, and track reads/writes.
    let mut kinds = [RegKind::Unknown; 256];
    let mut read_before_write = [false; 256];
    let mut written = [false; 256];
    let mut written_before_branch = [false; 256];
    let mut seen_branch = false;

    for (off, &op) in body.iter().enumerate() {
        if skip_mask[off] {
            continue;
        }
        let is_last = off + 1 == body.len();
        let mark_read = |r: u8,
                         kind: RegKind,
                         kinds: &mut [RegKind; 256],
                         rbw: &mut [bool; 256],
                         wr: &[bool; 256]| {
            let i = r as usize;
            kinds[i] = kinds[i].unify(kind);
            if !wr[i] {
                rbw[i] = true;
            }
        };
        let mark_write = |r: u8,
                          kind: RegKind,
                          kinds: &mut [RegKind; 256],
                          wr: &mut [bool; 256],
                          wbb: &mut [bool; 256],
                          seen_br: bool| {
            let i = r as usize;
            kinds[i] = kinds[i].unify(kind);
            wr[i] = true;
            if !seen_br {
                wbb[i] = true;
            }
        };

        match op.op {
            Op::AddF64 | Op::SubF64 | Op::MulF64 | Op::DivF64 => {
                mark_read(
                    op.b,
                    RegKind::Float,
                    &mut kinds,
                    &mut read_before_write,
                    &written,
                );
                mark_read(
                    op.c,
                    RegKind::Float,
                    &mut kinds,
                    &mut read_before_write,
                    &written,
                );
                mark_write(
                    op.a,
                    RegKind::Float,
                    &mut kinds,
                    &mut written,
                    &mut written_before_branch,
                    seen_branch,
                );
            }
            Op::AddF64Imm | Op::SubF64Imm | Op::MulF64Imm => {
                mark_read(
                    op.b,
                    RegKind::Float,
                    &mut kinds,
                    &mut read_before_write,
                    &written,
                );
                mark_write(
                    op.a,
                    RegKind::Float,
                    &mut kinds,
                    &mut written,
                    &mut written_before_branch,
                    seen_branch,
                );
            }
            Op::AddNum | Op::SubNum | Op::MulNum => {
                let k = if float_hints.get(op.b as usize).copied().unwrap_or(false)
                    || float_hints.get(op.c as usize).copied().unwrap_or(false)
                {
                    RegKind::Float
                } else {
                    RegKind::Int
                };
                mark_read(op.b, k, &mut kinds, &mut read_before_write, &written);
                mark_read(op.c, k, &mut kinds, &mut read_before_write, &written);
                mark_write(
                    op.a,
                    k,
                    &mut kinds,
                    &mut written,
                    &mut written_before_branch,
                    seen_branch,
                );
            }
            Op::AddImm | Op::SubImm | Op::MulImm => {
                let k = if float_hints.get(op.b as usize).copied().unwrap_or(false) {
                    RegKind::Float
                } else {
                    RegKind::Int
                };
                mark_read(op.b, k, &mut kinds, &mut read_before_write, &written);
                mark_write(
                    op.a,
                    k,
                    &mut kinds,
                    &mut written,
                    &mut written_before_branch,
                    seen_branch,
                );
            }
            Op::AddAssign | Op::SubAssign | Op::MulAssign => {
                let k = if float_hints.get(op.a as usize).copied().unwrap_or(false)
                    || float_hints.get(op.b as usize).copied().unwrap_or(false)
                {
                    RegKind::Float
                } else {
                    RegKind::Int
                };
                mark_read(op.a, k, &mut kinds, &mut read_before_write, &written);
                mark_read(op.b, k, &mut kinds, &mut read_before_write, &written);
                mark_write(
                    op.a,
                    k,
                    &mut kinds,
                    &mut written,
                    &mut written_before_branch,
                    seen_branch,
                );
            }
            Op::LoadF64 => {
                mark_write(
                    op.a,
                    RegKind::Float,
                    &mut kinds,
                    &mut written,
                    &mut written_before_branch,
                    seen_branch,
                );
            }
            Op::LoadInt => {
                mark_write(
                    op.a,
                    RegKind::Int,
                    &mut kinds,
                    &mut written,
                    &mut written_before_branch,
                    seen_branch,
                );
            }
            Op::Move | Op::Assign | Op::AssignTake | Op::NegNum => {
                let k = if kinds[op.b as usize] == RegKind::Float
                    || float_hints.get(op.b as usize).copied().unwrap_or(false)
                {
                    RegKind::Float
                } else {
                    RegKind::Int
                };
                mark_read(op.b, k, &mut kinds, &mut read_before_write, &written);
                mark_write(
                    op.a,
                    k,
                    &mut kinds,
                    &mut written,
                    &mut written_before_branch,
                    seen_branch,
                );
            }
            Op::CastFloat => {
                let k = if kinds[op.b as usize] == RegKind::Float
                    || float_hints.get(op.b as usize).copied().unwrap_or(false)
                {
                    RegKind::Float
                } else {
                    RegKind::Int
                };
                mark_read(op.b, k, &mut kinds, &mut read_before_write, &written);
                mark_write(
                    op.a,
                    RegKind::Float,
                    &mut kinds,
                    &mut written,
                    &mut written_before_branch,
                    seen_branch,
                );
            }
            Op::IncLocalFast | Op::DecLocalFast => {
                if float_hints.get(op.a as usize).copied().unwrap_or(false) {
                    return None;
                }
                mark_read(
                    op.a,
                    RegKind::Int,
                    &mut kinds,
                    &mut read_before_write,
                    &written,
                );
                mark_write(
                    op.a,
                    RegKind::Int,
                    &mut kinds,
                    &mut written,
                    &mut written_before_branch,
                    seen_branch,
                );
            }
            Op::Clear if op.a >= local_count => {}
            Op::CallNoop
                if op.c == 0
                    && op.a >= local_count
                    && temp_dead_in_loop(body, &skip_mask, off, op.a) => {}
            Op::BranchCmp => {
                let k = if kinds[op.b as usize] == RegKind::Float
                    || kinds[op.c as usize] == RegKind::Float
                    || float_hints.get(op.b as usize).copied().unwrap_or(false)
                    || float_hints.get(op.c as usize).copied().unwrap_or(false)
                {
                    RegKind::Float
                } else {
                    RegKind::Int
                };
                mark_read(op.b, k, &mut kinds, &mut read_before_write, &written);
                mark_read(op.c, k, &mut kinds, &mut read_before_write, &written);
                if !is_last {
                    seen_branch = true;
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
                let k = if kinds[op.a as usize] == RegKind::Float
                    || float_hints.get(op.a as usize).copied().unwrap_or(false)
                {
                    if matches!(op.op, Op::BranchIdentImm | Op::BranchNotIdentImm) {
                        return None;
                    }
                    RegKind::Float
                } else {
                    RegKind::Int
                };
                mark_read(op.a, k, &mut kinds, &mut read_before_write, &written);
                if !is_last {
                    seen_branch = true;
                }
            }
            Op::Jump if is_last => {}
            Op::IncLoopLtImm | Op::IncLoopLeImm | Op::DecLoopGeImm | Op::DecLoopGtImm
                if is_last =>
            {
                mark_read(
                    op.a,
                    RegKind::Int,
                    &mut kinds,
                    &mut read_before_write,
                    &written,
                );
                mark_write(
                    op.a,
                    RegKind::Int,
                    &mut kinds,
                    &mut written,
                    &mut written_before_branch,
                    seen_branch,
                );
            }
            Op::IncLoopLtReg | Op::IncLoopLeReg | Op::DecLoopGeReg | Op::DecLoopGtReg
                if is_last =>
            {
                mark_read(
                    op.a,
                    RegKind::Int,
                    &mut kinds,
                    &mut read_before_write,
                    &written,
                );
                mark_read(
                    op.b,
                    RegKind::Int,
                    &mut kinds,
                    &mut read_before_write,
                    &written,
                );
                mark_write(
                    op.a,
                    RegKind::Int,
                    &mut kinds,
                    &mut written,
                    &mut written_before_branch,
                    seen_branch,
                );
            }
            _ => return None,
        }
    }

    // Ensure no type conflicts. If a named local is written AFTER a branch
    // without being read before write (`$t` in `mandel2`), mark it live-in so
    // an early exit before the write restores its exact entry value!
    for r in 0..256 {
        if kinds[r] == RegKind::Conflict {
            return None;
        }
        if written[r]
            && r < local_count as usize
            && !read_before_write[r]
            && !written_before_branch[r]
        {
            read_before_write[r] = true;
        }
    }

    // 2. Assign dense variable slots in `iv` and `fv`.
    let mut int_regs: Vec<u8> = Vec::new();
    let mut f64_regs: Vec<u8> = Vec::new();
    let mut int_slot = [u8::MAX; 256];
    let mut f64_slot = [u8::MAX; 256];
    let mut live_in_ints: Vec<u8> = Vec::new();
    let mut live_in_f64s: Vec<u8> = Vec::new();
    let mut written_ints: Vec<u8> = Vec::new();
    let mut written_f64s: Vec<u8> = Vec::new();

    for r in 0..256 {
        match kinds[r] {
            RegKind::Int => {
                let s = int_regs.len();
                int_regs.push(r as u8);
                int_slot[r] = s as u8;
                if read_before_write[r] {
                    live_in_ints.push(s as u8);
                }
                if written[r] && (r < local_count as usize || written_before_branch[r]) {
                    written_ints.push(s as u8);
                }
            }
            RegKind::Float => {
                let s = f64_regs.len();
                f64_regs.push(r as u8);
                f64_slot[r] = s as u8;
                if read_before_write[r] {
                    live_in_f64s.push(s as u8);
                }
                if written[r] && (r < local_count as usize || written_before_branch[r]) {
                    written_f64s.push(s as u8);
                }
            }
            _ => {}
        }
    }

    let mut f64_consts: Vec<f64> = Vec::new();
    let mut i64_consts: Vec<i64> = Vec::new();

    let alloc_f64_const = |val: f64, f64_consts: &mut Vec<f64>| -> Option<u8> {
        let bits = val.to_bits();
        if let Some(pos) = f64_consts.iter().position(|x| x.to_bits() == bits) {
            return Some((f64_regs.len() + pos) as u8);
        }
        let slot = f64_regs.len() + f64_consts.len();
        if slot >= MAX_SCALAR_SLOTS {
            return None;
        }
        f64_consts.push(val);
        Some(slot as u8)
    };
    let alloc_i64_const = |val: i64, i64_consts: &mut Vec<i64>| -> Option<u8> {
        if let Some(pos) = i64_consts.iter().position(|&x| x == val) {
            return Some((int_regs.len() + pos) as u8);
        }
        let slot = int_regs.len() + i64_consts.len();
        if slot >= MAX_SCALAR_SLOTS {
            return None;
        }
        i64_consts.push(val);
        Some(slot as u8)
    };

    if int_regs.len() > MAX_SCALAR_SLOTS || f64_regs.len() > MAX_SCALAR_SLOTS {
        return None;
    }

    // Track `LoadF64` constants into single-use temps so `if ($Zr2 + $Zi2 > 4.0)`
    // and `2.0 * $Zr * $Zi + $Ci` fold cleanly!
    let mut temp_f64_const: [Option<f64>; 256] = [None; 256];
    for &op in body {
        if op.op == Op::LoadF64
            && (op.a as usize) >= local_count as usize
            && let Some(&c) = f64_pool.get(op.imm as usize)
        {
            temp_f64_const[op.a as usize] = Some(c);
        }
    }

    // 3. Special case: 2-op counter + accumulator loop (`simple`, `nestedloop`).
    if body.len() == 2 && body[0].op == Op::IncLocalFast && body[0].a != body[1].a {
        let a_slot = int_slot[body[0].a as usize];
        let i_slot = int_slot[body[1].a as usize];
        let fused = match body[1].op {
            Op::IncLoopLtImm => Some(ScalarOp::new(
                SOp::IncIncLoopLtImm,
                a_slot,
                i_slot,
                0,
                body[1].imm,
            )),
            Op::IncLoopLeImm => Some(ScalarOp::new(
                SOp::IncIncLoopLeImm,
                a_slot,
                i_slot,
                0,
                body[1].imm,
            )),
            Op::IncLoopLtReg => Some(ScalarOp::new(
                SOp::IncIncLoopLtReg,
                a_slot,
                i_slot,
                int_slot[body[1].b as usize],
                0,
            )),
            Op::IncLoopLeReg => Some(ScalarOp::new(
                SOp::IncIncLoopLeReg,
                a_slot,
                i_slot,
                int_slot[body[1].b as usize],
                0,
            )),
            _ => None,
        };
        if let Some(sop) = fused {
            return Some(ScalarLoop {
                ops: vec![sop].into_boxed_slice(),
                fast_ips: vec![body_start as u16].into_boxed_slice(),
                fallthrough_fast_ip: (end + 1) as u16,
                int_regs: int_regs.into_boxed_slice(),
                f64_regs: f64_regs.into_boxed_slice(),
                live_in_ints: live_in_ints.into_boxed_slice(),
                live_in_f64s: live_in_f64s.into_boxed_slice(),
                written_ints: written_ints.into_boxed_slice(),
                written_f64s: written_f64s.into_boxed_slice(),
                f64_consts: f64_consts.into_boxed_slice(),
                i64_consts: i64_consts.into_boxed_slice(),
            });
        }
    }

    // 3b. Special case: pure counter / dead-noop loop (`simpleucall`, `simpleudcall`).
    if body.len() >= 2
        && body[..body.len() - 1].iter().all(|op| {
            (op.op == Op::CallNoop && op.c == 0 && op.a >= local_count)
                || (op.op == Op::Clear && op.a >= local_count)
        })
        && matches!(body[body.len() - 1].op, Op::IncLoopLtImm | Op::IncLoopLeImm)
    {
        let last = body[body.len() - 1];
        let i_slot = int_slot[last.a as usize];
        let sop = if last.op == Op::IncLoopLtImm {
            ScalarOp::new(SOp::IncOnlyLoopLtImm, i_slot, 0, 0, last.imm)
        } else {
            ScalarOp::new(SOp::IncOnlyLoopLeImm, i_slot, 0, 0, last.imm)
        };
        return Some(ScalarLoop {
            ops: vec![sop].into_boxed_slice(),
            fast_ips: vec![body_start as u16].into_boxed_slice(),
            fallthrough_fast_ip: (end + 1) as u16,
            int_regs: int_regs.into_boxed_slice(),
            f64_regs: f64_regs.into_boxed_slice(),
            live_in_ints: live_in_ints.into_boxed_slice(),
            live_in_f64s: live_in_f64s.into_boxed_slice(),
            written_ints: written_ints.into_boxed_slice(),
            written_f64s: written_f64s.into_boxed_slice(),
            f64_consts: f64_consts.into_boxed_slice(),
            i64_consts: i64_consts.into_boxed_slice(),
        });
    }

    // 4. Lower body with general 2-op and 3-op peephole fusions using FINAL FastOp indices!
    let mut sops: Vec<ScalarOp> = Vec::with_capacity(body.len());
    let mut fast_ips: Vec<u16> = Vec::with_capacity(body.len());
    let mut k = 0usize;
    while k < body.len() {
        if skip_mask[k] {
            k += 1;
            continue;
        }
        let op = body[k];
        let op_fast_ip = (body_start + k) as u16;
        let next = body.get(k + 1).copied();
        let next2 = body.get(k + 2).copied();

        // Fusion A: `LoadF64(t1, 2.0) + MulF64(t2, t1, x) + MulF64(t3, t2, y) + AddF64(dst, t3, z)` -> `Mul2MulAddF64`!
        if op.op == Op::LoadF64
            && (op.a as usize) >= local_count as usize
            && f64_pool.get(op.imm as usize).copied() == Some(2.0)
            && let Some(op1) = next
            && op1.op == Op::MulF64
            && (op1.b == op.a || op1.c == op.a)
            && (op1.a as usize) >= local_count as usize
            && temp_dead_in_loop(body, &skip_mask, k + 1, op.a)
            && let Some(op2) = next2
            && op2.op == Op::MulF64
            && (op2.b == op1.a || op2.c == op1.a)
            && (op2.a as usize) >= local_count as usize
            && temp_dead_in_loop(body, &skip_mask, k + 2, op1.a)
            && let Some(op3) = body.get(k + 3).copied()
            && op3.op == Op::AddF64
            && (op3.b == op2.a || op3.c == op2.a)
            && temp_dead_in_loop(body, &skip_mask, k + 3, op2.a)
        {
            let x = if op1.b == op.a { op1.c } else { op1.b };
            let y = if op2.b == op1.a { op2.c } else { op2.b };
            let z = if op3.b == op2.a { op3.c } else { op3.b };
            sops.push(ScalarOp::new(
                SOp::Mul2MulAddF64,
                f64_slot[op3.a as usize],
                f64_slot[x as usize],
                f64_slot[y as usize],
                i32::from(f64_slot[z as usize]),
            ));
            fast_ips.push(op_fast_ip);
            k += 4;
            continue;
        }

        // Fusion B: `MulF64Imm(t1, x, 2) + MulF64(t2, t1, y) + AddF64(dst, t2, z)` -> `Mul2MulAddF64`!
        if op.op == Op::MulF64Imm
            && op.imm == 2
            && (op.a as usize) >= local_count as usize
            && let Some(op1) = next
            && op1.op == Op::MulF64
            && (op1.b == op.a || op1.c == op.a)
            && (op1.a as usize) >= local_count as usize
            && (op1.a == op.a || temp_dead_in_loop(body, &skip_mask, k + 1, op.a))
            && let Some(op2) = next2
            && op2.op == Op::AddF64
            && (op2.b == op1.a || op2.c == op1.a)
            && (op2.a == op1.a || temp_dead_in_loop(body, &skip_mask, k + 2, op1.a))
        {
            let x = op.b;
            let y = if op1.b == op.a { op1.c } else { op1.b };
            let z = if op2.b == op1.a { op2.c } else { op2.b };
            sops.push(ScalarOp::new(
                SOp::Mul2MulAddF64,
                f64_slot[op2.a as usize],
                f64_slot[x as usize],
                f64_slot[y as usize],
                i32::from(f64_slot[z as usize]),
            ));
            fast_ips.push(op_fast_ip);
            k += 3;
            continue;
        }

        // Fusion B2: `MulF64(t1, x, y) + MulF64Imm(t2, t1, 2) + AddF64(dst, t2, z)` -> `Mul2MulAddF64`!
        if op.op == Op::MulF64
            && (op.a as usize) >= local_count as usize
            && let Some(op1) = next
            && op1.op == Op::MulF64Imm
            && op1.imm == 2
            && op1.b == op.a
            && (op1.a as usize) >= local_count as usize
            && (op1.a == op.a || temp_dead_in_loop(body, &skip_mask, k + 1, op.a))
            && let Some(op2) = next2
            && op2.op == Op::AddF64
            && (op2.b == op1.a || op2.c == op1.a)
            && (op2.a == op1.a || temp_dead_in_loop(body, &skip_mask, k + 2, op1.a))
        {
            let z = if op2.b == op1.a { op2.c } else { op2.b };
            sops.push(ScalarOp::new(
                SOp::Mul2MulAddF64,
                f64_slot[op2.a as usize],
                f64_slot[op.b as usize],
                f64_slot[op.c as usize],
                i32::from(f64_slot[z as usize]),
            ));
            fast_ips.push(op_fast_ip);
            k += 3;
            continue;
        }

        // Fusion C: `AddF64(t, a, b) + LoadF64(tc, const) + BranchCmp(t, tc, exit)` -> `AddCmpF64Exit`!
        if op.op == Op::AddF64
            && (op.a as usize) >= local_count as usize
            && let Some(op1) = next
            && op1.op == Op::LoadF64
            && (op1.a as usize) >= local_count as usize
            && let Some(op2) = next2
            && op2.op == Op::BranchCmp
            && op2.b == op.a
            && op2.c == op1.a
            && temp_dead_in_loop(body, &skip_mask, k + 2, op.a)
            && temp_dead_in_loop(body, &skip_mask, k + 2, op1.a)
        {
            let c_val = *f64_pool.get(op1.imm as usize)?;
            let c_slot = alloc_f64_const(c_val, &mut f64_consts)?;
            let (exit_ip, flags, next_k) = if let Some((skip_off, early_ip)) =
                inline_early_exit(body, body_start, end, k + 2)
            {
                (early_ip as i32, i32::from(op2.a ^ 0x10), skip_off)
            } else {
                (op2.imm, i32::from(op2.a), k + 3)
            };
            sops.push(ScalarOp::new(
                SOp::AddCmpF64Exit,
                f64_slot[op.b as usize],
                f64_slot[op.c as usize],
                c_slot,
                exit_ip | (flags << 16),
            ));
            fast_ips.push(op_fast_ip);
            k = next_k;
            continue;
        }

        // Fusion D: `AddF64(t, a, b) + Branch*Imm(t, imm, exit)` -> `AddCmpF64Exit`!
        if op.op == Op::AddF64
            && (op.a as usize) >= local_count as usize
            && let Some(op1) = next
            && op1.a == op.a
            && let Some(cmp) = branch_imm_cmp_code(op1.op)
            && temp_dead_in_loop(body, &skip_mask, k + 1, op.a)
        {
            let c_slot = alloc_f64_const(op1.imm as f64, &mut f64_consts)?;
            let when = (op1.c & 0x80) != 0;
            let (target, eff_when, next_k) = if let Some((skip_off, early_ip)) =
                inline_early_exit(body, body_start, end, k + 1)
            {
                (early_ip, !when, skip_off)
            } else {
                (
                    (op1.b as usize) | (((op1.c & 0x7f) as usize) << 8),
                    when,
                    k + 2,
                )
            };
            let flags = i32::from(cmp | if eff_when { 0x10 } else { 0 });
            sops.push(ScalarOp::new(
                SOp::AddCmpF64Exit,
                f64_slot[op.b as usize],
                f64_slot[op.c as usize],
                c_slot,
                (target as i32) | (flags << 16),
            ));
            fast_ips.push(op_fast_ip);
            k = next_k;
            continue;
        }

        // Fusion E: `AddF64(t, a, b) + BranchCmp(t, c, exit)` -> `AddCmpF64Exit`!
        if op.op == Op::AddF64
            && (op.a as usize) >= local_count as usize
            && let Some(op1) = next
            && op1.op == Op::BranchCmp
            && op1.b == op.a
            && temp_dead_in_loop(body, &skip_mask, k + 1, op.a)
        {
            let (exit_ip, flags, next_k) = if let Some((skip_off, early_ip)) =
                inline_early_exit(body, body_start, end, k + 1)
            {
                (early_ip as i32, i32::from(op1.a ^ 0x10), skip_off)
            } else {
                (op1.imm, i32::from(op1.a), k + 2)
            };
            sops.push(ScalarOp::new(
                SOp::AddCmpF64Exit,
                f64_slot[op.b as usize],
                f64_slot[op.c as usize],
                f64_slot[op1.c as usize],
                exit_ip | (flags << 16),
            ));
            fast_ips.push(op_fast_ip);
            k = next_k;
            continue;
        }

        // Fusion F: `MulF64(t, a, b) + AddF64(dst, t, c)` -> `MulAddF64`!
        if op.op == Op::MulF64
            && (op.a as usize) >= local_count as usize
            && let Some(op1) = next
            && op1.op == Op::AddF64
            && (op1.b == op.a || op1.c == op.a)
            && (op1.a == op.a || temp_dead_in_loop(body, &skip_mask, k + 1, op.a))
        {
            let z = if op1.b == op.a { op1.c } else { op1.b };
            sops.push(ScalarOp::new(
                SOp::MulAddF64,
                f64_slot[op1.a as usize],
                f64_slot[op.b as usize],
                f64_slot[op.c as usize],
                i32::from(f64_slot[z as usize]),
            ));
            fast_ips.push(op_fast_ip);
            k += 2;
            continue;
        }

        // Fusion G: `SubF64(t, a, b) + AddF64(dst, t, c)` -> `SubAddF64`!
        if op.op == Op::SubF64
            && (op.a as usize) >= local_count as usize
            && let Some(op1) = next
            && op1.op == Op::AddF64
            && (op1.b == op.a || op1.c == op.a)
            && (op1.a == op.a || temp_dead_in_loop(body, &skip_mask, k + 1, op.a))
        {
            let z = if op1.b == op.a { op1.c } else { op1.b };
            sops.push(ScalarOp::new(
                SOp::SubAddF64,
                f64_slot[op1.a as usize],
                f64_slot[op.b as usize],
                f64_slot[op.c as usize],
                i32::from(f64_slot[z as usize]),
            ));
            fast_ips.push(op_fast_ip);
            k += 2;
            continue;
        }

        // Fusion H: `AddF64(t, a, b) + AddF64(dst, t, c)` -> `AddAddF64`!
        if op.op == Op::AddF64
            && (op.a as usize) >= local_count as usize
            && let Some(op1) = next
            && op1.op == Op::AddF64
            && (op1.b == op.a || op1.c == op.a)
            && (op1.a == op.a || temp_dead_in_loop(body, &skip_mask, k + 1, op.a))
        {
            let z = if op1.b == op.a { op1.c } else { op1.b };
            sops.push(ScalarOp::new(
                SOp::AddAddF64,
                f64_slot[op1.a as usize],
                f64_slot[op.b as usize],
                f64_slot[op.c as usize],
                i32::from(f64_slot[z as usize]),
            ));
            fast_ips.push(op_fast_ip);
            k += 2;
            continue;
        }

        // Standard single-op lowering inside `ScalarLoop`
        let sop = match op.op {
            Op::AddF64 => ScalarOp::new(
                SOp::AddF64,
                f64_slot[op.a as usize],
                f64_slot[op.b as usize],
                f64_slot[op.c as usize],
                0,
            ),
            Op::SubF64 => ScalarOp::new(
                SOp::SubF64,
                f64_slot[op.a as usize],
                f64_slot[op.b as usize],
                f64_slot[op.c as usize],
                0,
            ),
            Op::MulF64 => ScalarOp::new(
                SOp::MulF64,
                f64_slot[op.a as usize],
                f64_slot[op.b as usize],
                f64_slot[op.c as usize],
                0,
            ),
            Op::DivF64 => ScalarOp::new(
                SOp::DivF64,
                f64_slot[op.a as usize],
                f64_slot[op.b as usize],
                f64_slot[op.c as usize],
                0,
            ),
            Op::AddF64Imm => {
                let cs = alloc_f64_const(op.imm as f64, &mut f64_consts)?;
                ScalarOp::new(
                    SOp::AddF64,
                    f64_slot[op.a as usize],
                    f64_slot[op.b as usize],
                    cs,
                    0,
                )
            }
            Op::SubF64Imm => {
                let cs = alloc_f64_const(op.imm as f64, &mut f64_consts)?;
                ScalarOp::new(
                    SOp::SubF64,
                    f64_slot[op.a as usize],
                    f64_slot[op.b as usize],
                    cs,
                    0,
                )
            }
            Op::MulF64Imm => {
                let cs = alloc_f64_const(op.imm as f64, &mut f64_consts)?;
                ScalarOp::new(
                    SOp::MulF64,
                    f64_slot[op.a as usize],
                    f64_slot[op.b as usize],
                    cs,
                    0,
                )
            }
            Op::AddNum => match kinds[op.a as usize] {
                RegKind::Float => ScalarOp::new(
                    SOp::AddF64,
                    f64_slot[op.a as usize],
                    f64_slot[op.b as usize],
                    f64_slot[op.c as usize],
                    0,
                ),
                RegKind::Int => ScalarOp::new(
                    SOp::AddI64,
                    int_slot[op.a as usize],
                    int_slot[op.b as usize],
                    int_slot[op.c as usize],
                    0,
                ),
                _ => return None,
            },
            Op::SubNum => match kinds[op.a as usize] {
                RegKind::Float => ScalarOp::new(
                    SOp::SubF64,
                    f64_slot[op.a as usize],
                    f64_slot[op.b as usize],
                    f64_slot[op.c as usize],
                    0,
                ),
                RegKind::Int => ScalarOp::new(
                    SOp::SubI64,
                    int_slot[op.a as usize],
                    int_slot[op.b as usize],
                    int_slot[op.c as usize],
                    0,
                ),
                _ => return None,
            },
            Op::MulNum => match kinds[op.a as usize] {
                RegKind::Float => ScalarOp::new(
                    SOp::MulF64,
                    f64_slot[op.a as usize],
                    f64_slot[op.b as usize],
                    f64_slot[op.c as usize],
                    0,
                ),
                RegKind::Int => ScalarOp::new(
                    SOp::MulI64,
                    int_slot[op.a as usize],
                    int_slot[op.b as usize],
                    int_slot[op.c as usize],
                    0,
                ),
                _ => return None,
            },
            Op::AddImm => match kinds[op.a as usize] {
                RegKind::Float => {
                    let cs = alloc_f64_const(op.imm as f64, &mut f64_consts)?;
                    ScalarOp::new(
                        SOp::AddF64,
                        f64_slot[op.a as usize],
                        f64_slot[op.b as usize],
                        cs,
                        0,
                    )
                }
                RegKind::Int => ScalarOp::new(
                    SOp::AddImmI64,
                    int_slot[op.a as usize],
                    int_slot[op.b as usize],
                    0,
                    op.imm,
                ),
                _ => return None,
            },
            Op::SubImm => match kinds[op.a as usize] {
                RegKind::Float => {
                    let cs = alloc_f64_const(op.imm as f64, &mut f64_consts)?;
                    ScalarOp::new(
                        SOp::SubF64,
                        f64_slot[op.a as usize],
                        f64_slot[op.b as usize],
                        cs,
                        0,
                    )
                }
                RegKind::Int => ScalarOp::new(
                    SOp::SubImmI64,
                    int_slot[op.a as usize],
                    int_slot[op.b as usize],
                    0,
                    op.imm,
                ),
                _ => return None,
            },
            Op::MulImm => match kinds[op.a as usize] {
                RegKind::Float => {
                    let cs = alloc_f64_const(op.imm as f64, &mut f64_consts)?;
                    ScalarOp::new(
                        SOp::MulF64,
                        f64_slot[op.a as usize],
                        f64_slot[op.b as usize],
                        cs,
                        0,
                    )
                }
                RegKind::Int => ScalarOp::new(
                    SOp::MulImmI64,
                    int_slot[op.a as usize],
                    int_slot[op.b as usize],
                    0,
                    op.imm,
                ),
                _ => return None,
            },
            Op::AddAssign => match kinds[op.a as usize] {
                RegKind::Float => ScalarOp::new(
                    SOp::AddF64,
                    f64_slot[op.a as usize],
                    f64_slot[op.a as usize],
                    f64_slot[op.b as usize],
                    0,
                ),
                RegKind::Int => ScalarOp::new(
                    SOp::AddI64,
                    int_slot[op.a as usize],
                    int_slot[op.a as usize],
                    int_slot[op.b as usize],
                    0,
                ),
                _ => return None,
            },
            Op::SubAssign => match kinds[op.a as usize] {
                RegKind::Float => ScalarOp::new(
                    SOp::SubF64,
                    f64_slot[op.a as usize],
                    f64_slot[op.a as usize],
                    f64_slot[op.b as usize],
                    0,
                ),
                RegKind::Int => ScalarOp::new(
                    SOp::SubI64,
                    int_slot[op.a as usize],
                    int_slot[op.a as usize],
                    int_slot[op.b as usize],
                    0,
                ),
                _ => return None,
            },
            Op::MulAssign => match kinds[op.a as usize] {
                RegKind::Float => ScalarOp::new(
                    SOp::MulF64,
                    f64_slot[op.a as usize],
                    f64_slot[op.a as usize],
                    f64_slot[op.b as usize],
                    0,
                ),
                RegKind::Int => ScalarOp::new(
                    SOp::MulI64,
                    int_slot[op.a as usize],
                    int_slot[op.a as usize],
                    int_slot[op.b as usize],
                    0,
                ),
                _ => return None,
            },
            Op::LoadF64 => {
                let c_val = *f64_pool.get(op.imm as usize)?;
                let cs = alloc_f64_const(c_val, &mut f64_consts)?;
                ScalarOp::new(SOp::MovF64, f64_slot[op.a as usize], cs, 0, 0)
            }
            Op::LoadInt => {
                let cs = alloc_i64_const(i64::from(op.imm), &mut i64_consts)?;
                ScalarOp::new(SOp::MovI64, int_slot[op.a as usize], cs, 0, 0)
            }
            Op::Move | Op::Assign | Op::AssignTake => match kinds[op.a as usize] {
                RegKind::Float => ScalarOp::new(
                    SOp::MovF64,
                    f64_slot[op.a as usize],
                    f64_slot[op.b as usize],
                    0,
                    0,
                ),
                RegKind::Int => ScalarOp::new(
                    SOp::MovI64,
                    int_slot[op.a as usize],
                    int_slot[op.b as usize],
                    0,
                    0,
                ),
                _ => return None,
            },
            Op::NegNum => match kinds[op.a as usize] {
                RegKind::Float => ScalarOp::new(
                    SOp::NegF64,
                    f64_slot[op.a as usize],
                    f64_slot[op.b as usize],
                    0,
                    0,
                ),
                RegKind::Int => ScalarOp::new(
                    SOp::NegI64,
                    int_slot[op.a as usize],
                    int_slot[op.b as usize],
                    0,
                    0,
                ),
                _ => return None,
            },
            Op::CastFloat => match kinds[op.b as usize] {
                RegKind::Float => ScalarOp::new(
                    SOp::MovF64,
                    f64_slot[op.a as usize],
                    f64_slot[op.b as usize],
                    0,
                    0,
                ),
                RegKind::Int => ScalarOp::new(
                    SOp::I64ToF64,
                    f64_slot[op.a as usize],
                    int_slot[op.b as usize],
                    0,
                    0,
                ),
                _ => return None,
            },
            Op::IncLocalFast => ScalarOp::new(SOp::IncI64, int_slot[op.a as usize], 0, 0, 0),
            Op::DecLocalFast => ScalarOp::new(SOp::DecI64, int_slot[op.a as usize], 0, 0, 0),
            Op::Clear | Op::CallNoop => {
                k += 1;
                continue;
            }
            Op::BranchCmp => {
                if k + 1 == body.len() {
                    let sop = match kinds[op.b as usize] {
                        RegKind::Float => ScalarOp::new(
                            SOp::CmpF64Back,
                            f64_slot[op.b as usize],
                            f64_slot[op.c as usize],
                            op.a,
                            0,
                        ),
                        RegKind::Int => ScalarOp::new(
                            SOp::CmpI64Back,
                            int_slot[op.b as usize],
                            int_slot[op.c as usize],
                            op.a,
                            0,
                        ),
                        _ => return None,
                    };
                    sops.push(sop);
                    fast_ips.push(op_fast_ip);
                    k += 1;
                    continue;
                }
                let (exit_ip, flags, next_k) = if let Some((skip_off, early_ip)) =
                    inline_early_exit(body, body_start, end, k)
                {
                    (early_ip as i32, op.a ^ 0x10, skip_off)
                } else {
                    (op.imm, op.a, k + 1)
                };
                let sop = match kinds[op.b as usize] {
                    RegKind::Float => ScalarOp::new(
                        SOp::CmpF64Exit,
                        f64_slot[op.b as usize],
                        f64_slot[op.c as usize],
                        flags,
                        exit_ip,
                    ),
                    RegKind::Int => ScalarOp::new(
                        SOp::CmpI64Exit,
                        int_slot[op.b as usize],
                        int_slot[op.c as usize],
                        flags,
                        exit_ip,
                    ),
                    _ => return None,
                };
                sops.push(sop);
                fast_ips.push(op_fast_ip);
                k = next_k;
                continue;
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
                if k + 1 == body.len() {
                    let sop = match kinds[op.a as usize] {
                        RegKind::Float => {
                            let cs = alloc_f64_const(op.imm as f64, &mut f64_consts)?;
                            let flags = cmp | if when { 0x10 } else { 0 };
                            ScalarOp::new(SOp::CmpF64Back, f64_slot[op.a as usize], cs, flags, 0)
                        }
                        RegKind::Int => ScalarOp::new(
                            SOp::CmpI64ImmBack,
                            (int_slot[op.a as usize] & 31) | ((cmp & 7) << 5),
                            0,
                            if when { 0x80 } else { 0 },
                            op.imm,
                        ),
                        _ => return None,
                    };
                    sops.push(sop);
                    fast_ips.push(op_fast_ip);
                    k += 1;
                    continue;
                }
                let (target, eff_when, next_k) = if let Some((skip_off, early_ip)) =
                    inline_early_exit(body, body_start, end, k)
                {
                    (early_ip, !when, skip_off)
                } else {
                    (
                        (op.b as usize) | (((op.c & 0x7f) as usize) << 8),
                        when,
                        k + 1,
                    )
                };
                if target >= 32_768 {
                    return None;
                }
                let sop = match kinds[op.a as usize] {
                    RegKind::Float => {
                        let cs = alloc_f64_const(op.imm as f64, &mut f64_consts)?;
                        let flags = cmp | if eff_when { 0x10 } else { 0 };
                        ScalarOp::new(
                            SOp::CmpF64Exit,
                            f64_slot[op.a as usize],
                            cs,
                            flags,
                            target as i32,
                        )
                    }
                    RegKind::Int => ScalarOp::new(
                        SOp::CmpI64ImmExit,
                        (int_slot[op.a as usize] & 31) | ((cmp & 7) << 5),
                        (target & 0xff) as u8,
                        (((target >> 8) & 0x7f) as u8) | if eff_when { 0x80 } else { 0 },
                        op.imm,
                    ),
                    _ => return None,
                };
                sops.push(sop);
                fast_ips.push(op_fast_ip);
                k = next_k;
                continue;
            }
            Op::Jump => ScalarOp::new(SOp::JumpBack, 0, 0, 0, 0),
            Op::IncLoopLtImm => {
                ScalarOp::new(SOp::IncLoopLtImm, int_slot[op.a as usize], 0, 0, op.imm)
            }
            Op::IncLoopLeImm => {
                ScalarOp::new(SOp::IncLoopLeImm, int_slot[op.a as usize], 0, 0, op.imm)
            }
            Op::DecLoopGeImm => {
                ScalarOp::new(SOp::DecLoopGeImm, int_slot[op.a as usize], 0, 0, op.imm)
            }
            Op::DecLoopGtImm => {
                ScalarOp::new(SOp::DecLoopGtImm, int_slot[op.a as usize], 0, 0, op.imm)
            }
            Op::IncLoopLtReg => ScalarOp::new(
                SOp::IncLoopLtReg,
                int_slot[op.a as usize],
                int_slot[op.b as usize],
                0,
                0,
            ),
            Op::IncLoopLeReg => ScalarOp::new(
                SOp::IncLoopLeReg,
                int_slot[op.a as usize],
                int_slot[op.b as usize],
                0,
                0,
            ),
            Op::DecLoopGeReg => ScalarOp::new(
                SOp::DecLoopGeReg,
                int_slot[op.a as usize],
                int_slot[op.b as usize],
                0,
                0,
            ),
            Op::DecLoopGtReg => ScalarOp::new(
                SOp::DecLoopGtReg,
                int_slot[op.a as usize],
                int_slot[op.b as usize],
                0,
                0,
            ),
            _ => return None,
        };
        sops.push(sop);
        fast_ips.push(op_fast_ip);
        k += 1;
    }

    let _ = temp_f64_const;
    Some(ScalarLoop {
        ops: sops.into_boxed_slice(),
        fast_ips: fast_ips.into_boxed_slice(),
        fallthrough_fast_ip: (end + 1) as u16,
        int_regs: int_regs.into_boxed_slice(),
        f64_regs: f64_regs.into_boxed_slice(),
        live_in_ints: live_in_ints.into_boxed_slice(),
        live_in_f64s: live_in_f64s.into_boxed_slice(),
        written_ints: written_ints.into_boxed_slice(),
        written_f64s: written_f64s.into_boxed_slice(),
        f64_consts: f64_consts.into_boxed_slice(),
        i64_consts: i64_consts.into_boxed_slice(),
    })
}

fn temp_dead_in_loop(body: &[FastOp], skip_mask: &[bool; 65], after_k: usize, reg: u8) -> bool {
    for idx in (after_k + 1)..body.len() {
        if skip_mask[idx] {
            continue;
        }
        let op = body[idx];
        let reads = match op.op {
            Op::AddF64
            | Op::SubF64
            | Op::MulF64
            | Op::DivF64
            | Op::AddNum
            | Op::SubNum
            | Op::MulNum
            | Op::BranchCmp => op.b == reg || op.c == reg,
            Op::AddF64Imm
            | Op::SubF64Imm
            | Op::MulF64Imm
            | Op::AddImm
            | Op::SubImm
            | Op::MulImm
            | Op::Move
            | Op::Assign
            | Op::AssignTake
            | Op::NegNum
            | Op::CastFloat => op.b == reg,
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
            | Op::BranchNotIdentImm => op.a == reg,
            _ => false,
        };
        if reads {
            return false;
        }
        let writes = matches!(
            op.op,
            Op::AddF64
                | Op::SubF64
                | Op::MulF64
                | Op::DivF64
                | Op::AddF64Imm
                | Op::SubF64Imm
                | Op::MulF64Imm
                | Op::AddNum
                | Op::SubNum
                | Op::MulNum
                | Op::AddImm
                | Op::SubImm
                | Op::MulImm
                | Op::LoadF64
                | Op::LoadInt
                | Op::Move
                | Op::Assign
                | Op::AssignTake
                | Op::NegNum
                | Op::CastFloat
        ) && op.a == reg;
        if writes {
            return true;
        }
    }
    true
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

pub(crate) fn fast_op_branch_target(op: FastOp) -> Option<usize> {
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
        | Op::DecLoopGtReg => Some(op.imm as usize),
        Op::IncLoopLtImm | Op::IncLoopLeImm | Op::DecLoopGeImm | Op::DecLoopGtImm => {
            Some((op.b as usize) | ((op.c as usize) << 8))
        }
        Op::BranchLtImm
        | Op::BranchLeImm
        | Op::BranchGtImm
        | Op::BranchGeImm
        | Op::BranchEqImm
        | Op::BranchNeImm
        | Op::BranchIdentImm
        | Op::BranchNotIdentImm => Some((op.b as usize) | (((op.c & 0x7f) as usize) << 8)),
        _ => None,
    }
}
