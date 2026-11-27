//! Tier-2 compact register VM (`FastVm`) in 100% safe Rust.
//!
//! Hot PHP functions (loops and direct-call recursion) are lazily lowered from
//! 16-byte general bytecode into 8-byte `Copy` [`FastOp`] instructions running
//! on a zero-drop parallel `(tags: [u8], bits: [u64])` scalar register stack
//! with an out-of-line `heap` table for arrays, strings, and references.
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

/// Register value tag in [`FastStack::tags`].
pub(crate) const TAG_UNINIT: u8 = 0;
pub(crate) const TAG_NULL: u8 = 1;
pub(crate) const TAG_BOOL: u8 = 2;
pub(crate) const TAG_INT: u8 = 3;
pub(crate) const TAG_FLOAT: u8 = 4;
pub(crate) const TAG_HEAP: u8 = 5;

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
    // Constants, moves, clears
    LoadInt,
    LoadF64,
    LoadBool,
    LoadNull,
    LoadConstVal,
    NewArray,
    Move,
    Assign,
    AssignTake,
    Clear,

    // Numeric & bitwise binary (Int, Float, mixed Int/Float)
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

    // Immediate numeric & bitwise binary
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

    // Fused rotated-loop latches
    IncLoopLtImm,
    IncLoopLeImm,
    DecLoopGeImm,
    DecLoopGtImm,
    IncLoopLtReg,
    IncLoopLeReg,
    DecLoopGeReg,
    DecLoopGtReg,

    // In-loop array operations (COW & reference-aware)
    ArrGetInt,
    Arr2DGetInt,
    ArrSetInt,
    ArrAddAssignInt,
    ArrPush,
    ArrUnsetInt,

    // In-loop calls & returns
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
    pub(crate) f64_consts: Box<[f64]>,
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
