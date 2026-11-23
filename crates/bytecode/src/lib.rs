//! Phera's bytecode: what the compiler produces and the VM executes.
//!
//! A [`CompiledUnit`] is one PHP file. It holds a [`Function`] for the file's
//! top-level code and one for every function declared in it. A function is a
//! flat list of [`Instruction`]s that work on numbered slots called
//! registers.
//!
//! Three rules shape everything in this crate:
//!
//! 1. **Plain data.** A unit contains no PHP values and no `Rc`, so it is
//!    `Send + Sync` and one compiled copy is shared by every worker thread
//!    through an `Arc`. Nothing here changes after compilation.
//! 2. **Instructions are small and `Copy`.** The interpreter fetches one per
//!    step, so an instruction must never own heap memory. Anything of variable
//!    length (a name, an access path, a constant) lives in a table and the
//!    instruction carries an index into it. The size is checked at compile
//!    time.
//! 3. **Names are resolved by the compiler, not by the VM.** A local variable
//!    is a register number. A function, constant or type is an index into a
//!    table, already lowercased where PHP ignores case. The VM never parses or
//!    lowercases a string to run an instruction.
//!
//! `InitStaticCall` follows those rules: it contains only symbol indices and a
//! binding enum, remains Copy within the size assertion, and uses compiler-folded
//! method lookup spelling. `FetchIndex` likewise holds only registers, an
//! immutable fallback-path index and an access mode. Request-local resolution
//! stays in the VM's side caches.
//!
//! `Capture` holds only a source register and a value/reference/quiet policy.
//! `MakeClosure` holds a function index, destination and binding policy. Capture
//! names and the registers after parameters are assigned by the compiler; the
//! VM supplies request-local values and objects. Both instructions remain Copy
//! within the same size assertion as every other instruction.
//!
//! See `docs/architecture.md`, sections 4 and 9.3.

mod builtin;
mod class;
mod code;
mod display;
mod function;
mod ids;
mod instruction;
pub mod name_keys;
pub mod reflection;
mod unit;
mod unit_metadata;

pub use name_keys::NameKey;
pub use unit_metadata::{
    DeclarationScope, FunctionShape, FunctionSignature, UnitMetadata, class_keys,
};

pub use builtin::{BuiltinFrame, BuiltinUnary, CallbackArguments};
pub use class::{
    ClassConstant, ClassDeclaration, ClassKind, EnumBacking, EnumCase, Initializer,
    MethodDeclaration, PropertyDeclaration, PropertyHook, PropertyHooks, TRAIT_CLASS_NAME,
    TraitAdaptation, TraitUse, Visibility,
};
pub use code::Code;
pub use function::{
    ArgumentLayout, Function, Handler, MethodSite, Parameter, Path, PathBase, SiteArgument, Step,
};
pub use ids::{
    ClassId, ConstId, FunctionId, IterId, MethodSiteId, NameId, PathId, Reg, StaticId, SymbolId,
    Target, TypeId,
};
pub use instruction::{
    BinaryOp, CallResult, CaptureKind, CastKind, ClassBinding, ClosureBinding, FetchMode,
    FinallyEntry, IncDec, IncludeKind, Instruction, RuntimeError, UnaryOp,
};
pub use unit::{
    CompiledUnit, Constant, ConstantKey, DeclarationWarning, Symbol, Type, TypeMask,
    union_in_php_order,
};
