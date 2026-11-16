//! A compiled function and the tables its instructions point into.

use crate::{Instruction, NameId, Reg, Target, TypeId};

/// The code of one PHP function, or of a file's top level.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Function {
    /// The name as written, with its namespace: `App\helper`. `{main}` for a
    /// file's top-level code.
    pub name: String,
    pub instructions: Vec<Instruction>,
    /// The source line of each instruction, for error messages. Same length
    /// as `instructions`.
    pub lines: Vec<u32>,
    /// How many slots a frame for this function needs.
    pub registers: u16,
    /// The names of the first registers: parameters, then every other named
    /// variable the function mentions. Registers past these are temporaries.
    pub locals: Vec<NameId>,
    /// One entry per parameter; parameter `i` lives in register `i`.
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeId>,
    /// The access paths used by the path instructions.
    pub paths: Vec<Path>,
    /// The names of the function's `static` variables.
    pub statics: Vec<NameId>,
    /// How many `foreach` iterators a frame needs.
    pub iterators: u16,
    /// Whether variables can also be reached by name at run time. Set for
    /// top-level code and for functions that use `$$name`, `extract`,
    /// `compact`, `get_defined_vars`, `include` or `eval`. Such a frame keeps
    /// a name table next to its registers; all others skip that cost.
    pub dynamic_scope: bool,
    /// Preserve the mutable argument view for `func_get_args`, `func_get_arg`
    /// and `func_num_args`, including dynamically selected intrinsics.
    pub observes_arguments: bool,
    /// Whether the file declared `strict_types=1`, which governs the calls
    /// this function makes.
    pub strict_types: bool,
    /// `try` blocks, innermost first. Unwinding searches this table; it is
    /// empty until exceptions are compiled.
    pub handlers: Vec<Handler>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: NameId,
    pub ty: Option<TypeId>,
    pub by_reference: bool,
    /// `...$rest`: collects the remaining arguments into an array.
    pub variadic: bool,
    /// The code that computes the default follows the parameter's
    /// `ReceiveDefault` instruction.
    pub has_default: bool,
}

/// A place inside a variable: where it starts and how to walk from there.
/// `$a['x'][]` is base `$a`, then index `'x'`, then append.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub base: PathBase,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathBase {
    /// A register: a local variable, or a temporary holding a value.
    Register(Reg),
    /// `$$name`: the variable whose name is the string in this register.
    /// Only in functions with `dynamic_scope`.
    Named(Reg),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// `[key]`, with the key already evaluated into a register.
    Index(Reg),
    /// `[]`: the next free integer key. Only valid when writing.
    Append,
}

/// One `try` block: which instructions it covers and where control goes when
/// something is thrown inside them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Handler {
    /// First instruction covered.
    pub start: Target,
    /// First instruction no longer covered.
    pub end: Target,
    /// Where the `catch` clauses begin, if there are any.
    pub catch: Option<Target>,
    /// Where the `finally` block begins, if there is one.
    pub finally: Option<Target>,
}
