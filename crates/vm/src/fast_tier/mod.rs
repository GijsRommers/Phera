//! Tier-2 compact register VM (`FastVm`) in 100% safe Rust.
//!
//! Hot PHP functions (functions with a Tier-2-clean loop or clean direct
//! recursion) are lazily lowered from 16-byte general bytecode into 8-byte
//! `Copy` [`FastOp`] instructions running on raw `(tags: [u8; 256], bits: [u64; 256])`
//! register windows (zero bounds checks, raw 64-bit `i64`/`f64`, zero-alloc
//! `TAG_CONST` constants). Pure-scalar inner loops are further specialized into
//! tagless stack-array [`scalar_loop::ScalarLoop`] kernels, and pure-integer
//! self-recursive functions into raw-`i64` [`int_rec::IntRecFunc`] kernels.

pub(crate) mod exec;
pub(crate) mod int_rec;
pub(crate) mod list_loop;
pub(crate) mod lower;
pub(crate) mod scalar_loop;
pub(crate) mod stack;

use std::sync::Arc;
use std::sync::atomic::AtomicU8;

use phera_bytecode::TypeMask;

use crate::calls::execution_loop::Position;
use crate::code_table::FunctionIndex;

pub(crate) use stack::FastStack;

/// Dense register tag values in `FastStack::tags`.
pub(crate) const TAG_UNINIT: u8 = 0;
pub(crate) const TAG_NULL: u8 = 1;
pub(crate) const TAG_BOOL: u8 = 2;
pub(crate) const TAG_INT: u8 = 3;
pub(crate) const TAG_FLOAT: u8 = 4;
/// Immutable unit constant by `(unit_cache_const_id: u32)` — zero alloc/drop!
pub(crate) const TAG_CONST: u8 = 5;
/// Heap slot index (`u32`) into `FastStack::heap` — ONLY tag that may own
/// memory or hold a PHP reference!
pub(crate) const TAG_HEAP: u8 = 6;

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
    // Specialized unboxed inner-loop entry (guarded fallback to tagged ops)
    EnterScalarLoop,
    EnterListLoop,

    // Fused rotated-loop latches
    IncLoopLtImm,
    IncLoopLeImm,
    DecLoopGeImm,
    DecLoopGtImm,
    IncLoopLtReg,
    IncLoopLeReg,
    DecLoopGeReg,
    DecLoopGtReg,

    // Float-first arithmetic
    AddF64,
    SubF64,
    MulF64,
    DivF64,
    MulF64Imm,
    AddF64Imm,
    SubF64Imm,

    // Int-first arithmetic
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

    // Immediate integer arithmetic
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
    MulAddAssign,
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
    /// `echo` of a string into [`FastStack::echo`](crate::fast_tier::FastStack),
    /// when the VM opened it for this run.
    Echo,

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

    // In-loop array operations (direct `Storage::List` fast paths + COW/ref safe)
    ArrGetInt,
    Arr2DGetInt,
    ArrSetInt,
    Arr2DSetInt,
    ArrAddAssignInt,
    ArrAddFromArr,
    ArrAddFromArrLoop,
    Arr2DMulAddLoop,
    ArrUnsetStepLoop,
    ArrSetRegStepLoop,
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
    pub(crate) f64_consts: Box<[f64]>,
    /// Specialized unboxed inner scalar loops referenced by `Op::EnterScalarLoop`.
    pub(crate) scalar_loops: Box<[scalar_loop::ScalarLoop]>,
    /// Specialized hoisted single-list loops referenced by `Op::EnterListLoop`.
    pub(crate) list_loops: Box<[list_loop::ListLoop]>,
    /// Optional pure-`i64` self-recursion kernel (`fibo_r`, `Ack`, etc.).
    pub(crate) int_rec: Option<int_rec::IntRecFunc>,
    /// Count of runtime type-deopts; after 2 strikes the function is disabled.
    pub(crate) type_deopts: AtomicU8,
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
    Deopt {
        position: Position,
        yield_now: bool,
        /// When `Some`, `CodeTable::mark_fast_ineligible` should disable this
        /// function after repeated runtime type-deopts.
        disable_fn: Option<FunctionIndex>,
    },
}
