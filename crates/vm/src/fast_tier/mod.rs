//! Tier-2 compact register VM (`FastVm`) in 100% safe Rust.
//!
//! Hot PHP functions (functions with a Tier-2-clean loop or clean direct
//! recursion) are lazily lowered from 16-byte general bytecode into 8-byte
//! `Copy` [`FastOp`] instructions running on a zero-drop 8-byte NaN-boxed
//! `u64` register stack (`&mut [u64; 256]` fixed windows — zero bounds checks
//! by type!) with an out-of-line `heap` table only for mutable arrays,
//! non-const strings, and references.
//!
//! Any unsupported operation, overflow, division by zero, reference write, or
//! type mismatch deoptimizes cleanly before mutating observable state,
//! materializing all active nested [`FastFrame`]s back into `Vm::registers`
//! and `Vm::call_stack` at the exact original bytecode instruction pointer.

pub(crate) mod exec;
pub(crate) mod lower;
pub(crate) mod stack;

use std::sync::Arc;

use phera_bytecode::TypeMask;

use crate::calls::execution_loop::Position;

pub(crate) use stack::FastStack;

// =============================================================================
// 100% Safe-Rust 8-Byte NaN-Boxing Representation (`u64`)
// =============================================================================
// Any `w < TAG_BASE` is an IEEE-754 `f64` (including ±Inf and canonical NaN
// `0x7ff8_0000_0000_0000`). Bit patterns `>= TAG_BASE` carry a 16-bit tag in
// bits 48..64 and a 48-bit payload in bits 0..48.
pub(crate) const CANON_NAN: u64 = 0x7ff8_0000_0000_0000;
pub(crate) const TAG_BASE: u64 = 0xfff9_0000_0000_0000;
pub(crate) const TAG_MASK: u64 = 0xffff_0000_0000_0000;
pub(crate) const PAYLOAD_MASK: u64 = 0x0000_ffff_ffff_ffff;

/// 48-bit signed integer (`[-140_737_488_355_328, +140_737_488_355_327]`).
/// Integers outside 48 bits are stored in `TAG_HEAP` (`Value::Int(i64)`).
pub(crate) const TAG_INT: u64 = 0xfff9_0000_0000_0000;
/// PHP `null`.
pub(crate) const TAG_NULL: u64 = 0xfffa_0000_0000_0000;
/// PHP `bool` (payload bit 0 is `0` or `1`).
pub(crate) const TAG_BOOL: u64 = 0xfffb_0000_0000_0000;
/// Immutable unit constant by `(unit_cache_const_id: u32)` — zero alloc/drop!
pub(crate) const TAG_CONST: u64 = 0xfffc_0000_0000_0000;
/// Unassigned / undefined slot.
pub(crate) const TAG_UNINIT: u64 = 0xfffd_0000_0000_0000;
/// Heap slot index (`u32`) into `FastStack::heap` — ONLY tag that may own
/// memory or hold a PHP reference!
pub(crate) const TAG_HEAP: u64 = 0xfffe_0000_0000_0000;

#[inline(always)]
pub(crate) fn box_f64(f: f64) -> u64 {
    let b = f.to_bits();
    if b >= TAG_BASE { CANON_NAN } else { b }
}

#[inline(always)]
pub(crate) fn is_f64(w: u64) -> bool {
    w < TAG_BASE
}

#[inline(always)]
pub(crate) fn unbox_f64(w: u64) -> f64 {
    f64::from_bits(w)
}

#[inline(always)]
pub(crate) fn is_int(w: u64) -> bool {
    (w & TAG_MASK) == TAG_INT
}

#[inline(always)]
pub(crate) fn unbox_int(w: u64) -> i64 {
    ((w as i64) << 16) >> 16
}

#[inline(always)]
pub(crate) fn box_int_checked(x: i64) -> Option<u64> {
    if ((x << 16) >> 16) == x {
        Some(TAG_INT | ((x as u64) & PAYLOAD_MASK))
    } else {
        None
    }
}

#[inline(always)]
pub(crate) fn box_i32(x: i32) -> u64 {
    TAG_INT | ((i64::from(x) as u64) & PAYLOAD_MASK)
}

#[inline(always)]
pub(crate) fn box_bool(b: bool) -> u64 {
    TAG_BOOL | u64::from(b)
}

/// Comparison sub-opcodes encoded in `FastOp::a` (low 4 bits) or `FastOp::imm`.
pub(crate) const CMP_EQ: u8 = 0;
pub(crate) const CMP_NE: u8 = 1;
pub(crate) const CMP_IDENT: u8 = 2;
pub(crate) const CMP_NOT_IDENT: u8 = 3;
pub(crate) const CMP_LT: u8 = 4;
pub(crate) const CMP_LE: u8 = 5;
pub(crate) const CMP_GT: u8 = 6;
pub(crate) const CMP_GE: u8 = 7;

/// Compact 8-byte Tier-2 instruction (`8` instructions per 64-byte cache line).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct FastOp {
    pub(crate) op: Op,
    pub(crate) a: u8,
    pub(crate) b: u8,
    pub(crate) c: u8,
    pub(crate) imm: i32,
}

const _: () = assert!(std::mem::size_of::<FastOp>() == 8);

impl FastOp {
    #[inline(always)]
    pub(crate) const fn new(op: Op, a: u8, b: u8, c: u8, imm: i32) -> Self {
        Self { op, a, b, c, imm }
    }
}

/// Dense 1-byte Tier-2 opcode table.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    // Fused rotated-loop latches
    IncLoopLtImm,
    IncLoopLeImm,
    DecLoopGeImm,
    DecLoopGtImm,
    IncLoopLtReg,
    IncLoopLeReg,
    DecLoopGeReg,
    DecLoopGtReg,

    // Float-first arithmetic (for float-typed expressions like mandel/mandel2/heapsort)
    AddF64,
    SubF64,
    MulF64,
    DivF64,
    MulF64Imm,
    AddF64Imm,
    SubF64Imm,

    // Int-first arithmetic (with out-of-line float/overflow fallback)
    AddNum,
    SubNum,
    MulNum,
    DivNum,
    ModInt,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,

    // Immediate integer arithmetic (with out-of-line float/overflow fallback)
    AddImm,
    SubImm,
    MulImm,
    ModImm,
    BitAndImm,
    BitOrImm,
    BitXorImm,
    ShlImm,
    ShrImm,

    // Compound local updates
    AddAssign,
    SubAssign,
    MulAssign,
    StrAppend,

    // Constants, moves, clears
    LoadInt,
    LoadF64,
    LoadBool,
    LoadNull,
    LoadConstRef,
    NewArray,
    Move,
    Assign,
    AssignTake,
    Clear,

    // Unary, casts, builtins
    NegNum,
    NotBool,
    BitNotInt,
    CastInt,
    CastFloat,
    CastBool,
    StrLen,

    // Inc/Dec
    IncLocalFast,
    DecLocalFast,
    IncDecLocal,

    // Comparisons & branches
    CmpNum,
    Jump,
    JumpIf,
    JumpIfNot,
    JumpIfNull,
    JumpIfNotNull,
    BranchCmp,
    BranchLtImm,
    BranchLeImm,
    BranchGtImm,
    BranchGeImm,
    BranchEqImm,
    BranchNeImm,
    BranchIdentImm,
    BranchNotIdentImm,

    // In-loop array operations (COW & reference-aware, out-of-line helpers)
    ArrGetInt,
    Arr2DGetInt,
    ArrSetInt,
    ArrAddAssignInt,
    ArrAddFromArr,
    ArrPush,
    ArrUnsetInt,

    // In-loop direct calls & returns
    CallSelf1,
    CallSelf2,
    CallNoop,
    CallFast,
    ReturnReg,
    RetIntImm,
    ReturnNull,

    // Exact deoptimization escape hatch
    Deopt,
}

/// Lazily compiled Tier-2 representation of a single PHP function.
pub(crate) struct FastFunction {
    pub(crate) ops: Box<[FastOp]>,
    /// Original bytecode IP for each `FastOp`, consulted only on deopt/yield.
    pub(crate) bc_ip: Box<[u32]>,
    /// Original bytecode IP -> `FastOp` index (`u16::MAX` when fused/invalid).
    pub(crate) bc_to_fast: Box<[u16]>,
    /// Flat float constant pool indexed by `Op::LoadF64::imm`.
    pub(crate) f64_consts: Box<[u64]>,
    pub(crate) reg_count: u8,
    pub(crate) local_count: u8,
    pub(crate) param_count: u8,
    pub(crate) typed_params: u64,
    pub(crate) param_masks: Arc<[TypeMask]>,
    pub(crate) return_mask: Option<TypeMask>,
    pub(crate) takes_values_only: bool,
    pub(crate) is_plain: bool,
    pub(crate) is_empty_return_null: bool,
}

/// Cached compilation status of a `FunctionIndex` inside `CodeTable`.
#[derive(Clone)]
pub(crate) enum FastEntry {
    Unchecked,
    Ineligible,
    Compiled(Arc<FastFunction>),
}

/// Outcome of running a burst in `FastVm`.
pub(crate) enum FastExit {
    /// Entry frame returned cleanly to its caller in `Machine::run`.
    Returned(Position),
    /// Deoptimized back to `Vm::registers`/`Vm::call_stack` at `position`.
    /// When `yield_now` is true, `Machine::run` yields immediately.
    Deopt { position: Position, yield_now: bool },
}
