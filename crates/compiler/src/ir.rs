//! Compiler-only lowering representation. Paths are interned before code leaves this crate.

use phera_bytecode::Reg;
/// An IR instruction offset, translated to a bytecode offset during emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target(pub usize);
/// Literal data before interning into the compiled unit constant table.
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    EmptyArray,
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Bytes(Vec<u8>),
}
/// Supported binary operations whose operands have already been evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binary {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
    Concat,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    ShiftLeft,
    ShiftRight,
    Xor,
    Equal,
    NotEqual,
    Identical,
    NotIdentical,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Spaceship,
}
/// Unary operations and casts before conversion to bytecode opcode families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unary {
    Plus,
    Minus,
    Not,
    BitwiseNot,
    Bool,
    Int,
    Float,
    String,
    Array,
}
/// Selects PHP's local, variable-variable, or request-global namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceScope {
    Local,
    Dynamic,
    Global(Reg),
    Globals,
}
/// An evaluated variable root and ordered index steps for a read or write.
#[derive(Debug, Clone)]
pub struct Place {
    pub base: Reg,
    pub scope: PlaceScope,
    pub path: Vec<Option<Reg>>,
}
/// Values and writable argument paths are distinct; a path needs no dummy register.
#[derive(Debug, Clone)]
pub enum ArgumentSource {
    Value(Reg),
    Place(Place),
}

/// An evaluated call argument with optional reference-capable place and name.
#[derive(Debug, Clone)]
pub struct Argument {
    pub source: ArgumentSource,
    pub name: Option<String>,
    pub spread: bool,
}
/// Compiler IR with owned names and paths before table interning.
#[derive(Debug, Clone)]
pub enum Instruction {
    Check {
        local: Reg,
        ty: String,
    },
    DynamicGlobal {
        name: Reg,
    },
    Clear {
        register: Reg,
    },
    DefaultCheck {
        param: Reg,
        passed: Target,
    },
    StaticCheck {
        local: Reg,
        name: String,
        initialized: Target,
    },
    StaticInit {
        local: Reg,
        name: String,
        src: Reg,
    },
    StartCall {
        name: String,
        fallback: Option<String>,
    },
    StartDynamicCall {
        callee: Reg,
    },
    Send {
        argument: Argument,
    },
    FinishCall {
        dst: Reg,
    },
    FinishCallable {
        dst: Reg,
    },
    IncPlace {
        dst: Reg,
        place: Place,
        mode: phera_bytecode::IncDec,
    },
    Compound {
        dst: Reg,
        place: Place,
        operator: Binary,
        source: Reg,
    },

    Reference {
        dst: Reg,
        place: Place,
    },
    Rebind {
        place: Place,
        src: Reg,
    },
    Include {
        dst: Reg,
        path: Reg,
        once: bool,
        required: bool,
    },
    Load {
        dst: Reg,
        constant: usize,
    },
    Move {
        dst: Reg,
        src: Reg,
    },
    Read {
        dst: Reg,
        place: Place,
        mode: phera_bytecode::FetchMode,
    },
    Write {
        place: Place,
        src: Reg,
    },
    Bind {
        place: Place,
        source: Place,
    },
    Unset(Place),
    Binary {
        dst: Reg,
        operator: Binary,
        lhs: Reg,
        rhs: Reg,
    },
    Unary {
        dst: Reg,
        operator: Unary,
        src: Reg,
    },
    Echo(Reg),
    Jump(Target),
    Branch {
        value: Reg,
        target: Target,
        when: bool,
    },
    BranchNull {
        value: Reg,
        target: Target,
    },
    Call {
        dst: Reg,
        name: Reg,
        fallback: Option<String>,
        arguments: Vec<Argument>,
    },
    Return(Reg),
    ReturnReference(Reg),
    Exit(Reg),
    Suppress(bool),
    Array(Reg),
    ArrayPush {
        array: Reg,
        key: Option<Reg>,
        value: Reg,
        spread: bool,
    },
    Constant {
        dst: Reg,
        name: String,
        fallback: Option<String>,
    },
    Define {
        name: String,
        src: Reg,
    },
    Declare(usize),
    Global {
        local: Reg,
        name: String,
    },
    IterInit {
        id: Reg,
        src: Reg,
        place: Option<Place>,
    },
    IterNext {
        id: Reg,
        key: Reg,
        value: Reg,
        end: Target,
    },
    MatchError {
        subject: Reg,
    },
}
/// A function parameter and the metadata needed by the runtime call protocol.
#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub has_default: bool,
    pub ty: Option<String>,
    pub by_reference: bool,
    pub variadic: bool,
}
/// One function of IR plus its constants, locals, source lines and scope requirements.
#[derive(Debug, Clone, Default)]
pub struct Function {
    pub name: String,
    pub definition_line: usize,
    pub instructions: Vec<Instruction>,
    pub constants: Vec<Constant>,
    pub registers: usize,
    pub parameters: Vec<Parameter>,
    pub locals: Vec<String>,
    pub lines: Vec<usize>,
    pub dynamic_scope: bool,
    pub strict_types: bool,
    pub return_type: Option<String>,
    pub returns_reference: bool,
}
/// The top-level function and declarations awaiting final bytecode emission.
#[derive(Debug, Clone)]
pub struct CompiledUnit {
    pub main: Function,
    pub functions: Vec<Function>,
    pub hoisted: Vec<usize>,
    pub file: String,
}
