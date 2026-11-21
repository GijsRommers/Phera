//! A compiled function and the tables its instructions point into.

use crate::{Code, NameId, Reg, Target, TypeId};

/// The code of one PHP function, or of a file's top level.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Function {
    /// The name as written, with its namespace: `App\helper`. `{main}` for a
    /// file's top-level code.
    pub name: String,
    /// Declaration location for parameter errors raised before the body executes.
    pub definition_line: u32,
    pub instructions: Code,
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
    /// Lexical captures follow parameters and precede ordinary locals.
    pub captures: u16,
    pub return_type: Option<TypeId>,
    /// Return aliases are part of an inherited method contract.
    pub returns_reference: bool,
    /// The declared result type describes the Generator object, not its final return value.
    pub is_generator: bool,
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
    /// Fixed signatures retain surplus arguments for traces; variadics store them in a local.
    pub argument_layout: ArgumentLayout,
    /// Whether the file declared `strict_types=1`, which governs the calls
    /// this function makes.
    pub strict_types: bool,
    /// `try` blocks, innermost first. Unwinding searches this table; it is
    /// consulted only when an exception leaves an instruction.
    pub handlers: Vec<Handler>,
    /// The method calls that `CallMethod` makes in one instruction.
    pub method_sites: Vec<MethodSite>,
    /// The arguments of every method site, one site after another; a site
    /// names its run of them.
    pub site_arguments: Vec<SiteArgument>,
}

/// `$order->total($tax, 2)` for `CallMethod`: the receiver, the method name
/// and the arguments, all locals or literals, so that nothing PHP could
/// observe happens between finding the method and reading them.
///
/// The call written out the general way (`InitMethodCall`, one `Send` per
/// argument, `Call`) follows `CallMethod`, and runs whenever the quick
/// entry declines (a native method, `__call`, a by-reference parameter, an
/// argument its type test refuses). `after` is the instruction after its
/// `Call`, where a call the quick entry made returns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MethodSite {
    pub receiver: Reg,
    pub method: NameId,
    /// The first of the site's entries in [`Function::site_arguments`].
    pub first_argument: u32,
    pub arguments: u16,
    pub after: Target,
}

impl MethodSite {
    /// Where the site's arguments are in [`Function::site_arguments`].
    pub fn argument_range(&self) -> std::ops::Range<usize> {
        let first = self.first_argument as usize;
        first..first + self.arguments as usize
    }
}

/// One argument of a method site: what the general sequence would send.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SiteArgument {
    /// `SendLocal`: a named local, which a by-reference parameter binds to.
    Local(Reg),
    /// `SendValue`: a temporary holding a literal.
    Value(Reg),
}

impl SiteArgument {
    pub fn register(self) -> Reg {
        match self {
            SiteArgument::Local(register) | SiteArgument::Value(register) => register,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    /// Written default spelling retained for method compatibility diagnostics.
    pub default_display: Option<String>,
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
    /// The active instance receiver is materialized only for path execution.
    This { scratch: Reg },
    /// Static storage is shared by inherited declarations and aliased through a scratch slot.
    StaticProperty { class: Reg, name: Reg, scratch: Reg },
    /// `Post::$count`: a static property of a class the source names, with
    /// its name written in the source; the class is found by its symbol's
    /// name key.
    NamedStaticProperty {
        class: crate::SymbolId,
        name: crate::NameId,
        scratch: Reg,
    },
    /// A register: a local variable, or a temporary holding a value.
    Register(Reg),
    /// `$$name`: the variable whose name is the string in this register.
    /// Only in functions with `dynamic_scope`.
    Named(Reg),
    /// Request-global variable whose name has already been evaluated.
    GlobalNamed { name: Reg, scratch: Reg },
    /// A copy of PHP's global symbol table; only valid for reads.
    Globals,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// A declared property name, interned in the unit name table.
    Property(NameId),
    /// An evaluated property name; numeric names remain strings.
    DynamicProperty(Reg),
    /// `[key]`, with the key already evaluated into a register.
    Index(Reg),
    /// `[]`: the next free integer key. Only valid when writing.
    Append,
}

/// One `try` block: which instructions it covers and where control goes when
/// something is thrown inside them.
#[derive(Debug, Clone, PartialEq)]
pub struct Handler {
    /// First instruction covered.
    pub start: Target,
    /// First instruction no longer covered.
    pub end: Target,
    /// Where the `catch` clauses begin, if there are any.
    pub catch: Option<Target>,
    /// Where the `finally` block begins, if there is one.
    pub finally: Option<Target>,
    /// Catch operand or finally continuation register, allocated by the compiler.
    pub exception: Reg,
    /// Abandoned expression temporaries; named locals survive the transfer.
    pub cleanup: Vec<Reg>,
}

/// Precomputed call arity policy avoids inspecting the last parameter on every invocation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ArgumentLayout {
    #[default]
    Fixed,
    Variadic,
}
