//! Owned operands and function metadata used while building compiler IR.

use super::Instruction;
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
/// A function parameter and the metadata needed by the runtime call protocol.
#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub has_default: bool,
    pub declaration_type: Option<String>,
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
