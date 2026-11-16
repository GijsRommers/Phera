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
    Object,
}
/// Selects PHP's local, variable-variable, or request-global namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceScope {
    /// The base register is a scratch slot for the current receiver.
    This,
    /// The base register holds a class name or object.
    StaticProperty {
        name: Reg,
        scratch: Reg,
    },
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
    pub path: Vec<PathStep>,
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
    pub classes: Vec<ClassDeclaration>,
    pub hoisted_classes: Vec<usize>,
    pub file: String,
}

/// Owned class names are interned only after method functions have their final indices.
#[derive(Debug, Clone)]
pub struct ClassDeclaration {
    pub name: String,
    pub parent: Option<String>,
    pub interfaces: Vec<String>,
    pub kind: phera_bytecode::ClassKind,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_readonly: bool,
    pub allows_dynamic_properties: bool,
    pub constants: Vec<ClassConstant>,
    pub properties: Vec<ClassProperty>,
    pub methods: Vec<ClassMethod>,
}
/// Method metadata travels beside ordinary function IR rather than entering global scope.
#[derive(Debug, Clone)]
pub struct ClassMethod {
    pub name: String,
    pub function: usize,
    pub visibility: phera_bytecode::Visibility,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_final: bool,
}

/// Property metadata keeps source names until bytecode interning.
#[derive(Debug, Clone)]
pub struct ClassProperty {
    pub name: String,
    pub visibility: phera_bytecode::Visibility,
    pub is_static: bool,
    pub is_readonly: bool,
    pub ty: Option<String>,
    pub default: Option<usize>,
}

/// Constant initializers execute only when the linked class first needs their value.
#[derive(Debug, Clone)]
pub struct ClassConstant {
    pub name: String,
    pub visibility: phera_bytecode::Visibility,
    pub is_final: bool,
    pub initializer: usize,
    pub ty: Option<String>,
}

/// Literal property names are interned once; computed names retain evaluated registers.
#[derive(Debug, Clone)]
pub enum PathStep {
    Index(Reg),
    Append,
    Property(String),
    DynamicProperty(Reg),
}
