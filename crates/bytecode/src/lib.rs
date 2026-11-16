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
//! See `docs/architecture.md`, section 4.

mod class;
mod display;
mod function;
mod ids;
mod instruction;
mod unit;

pub use class::{
    ClassConstant, ClassDeclaration, ClassKind, Initializer, MethodDeclaration,
    PropertyDeclaration, Visibility,
};
pub use function::{Function, Handler, Parameter, Path, PathBase, Step};
pub use ids::{
    ClassId, ConstId, FunctionId, IterId, NameId, PathId, Reg, StaticId, SymbolId, Target, TypeId,
};
pub use instruction::{
    BinaryOp, CastKind, ClassBinding, FetchMode, IncDec, IncludeKind, Instruction, RuntimeError,
    UnaryOp,
};
pub use unit::{CompiledUnit, Constant, ConstantKey, Symbol, Type, TypeMask};
