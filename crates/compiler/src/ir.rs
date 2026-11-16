//! Compiler-only lowering representation. Paths are interned before code leaves this crate.
use phera_bytecode::Reg;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target(pub usize);
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    EmptyArray,
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Bytes(Vec<u8>),
}
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
#[derive(Debug, Clone)]
pub struct Place {
    pub base: Reg,
    pub dynamic: bool,
    pub path: Vec<Option<Reg>>,
}
#[derive(Debug, Clone)]
pub struct Argument {
    pub value: Reg,
    pub place: Option<Place>,
    pub name: Option<String>,
    pub spread: bool,
}
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
        increment: bool,
        prefix: bool,
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
        quiet: bool,
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
#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub has_default: bool,
    pub ty: Option<String>,
    pub by_reference: bool,
    pub variadic: bool,
}
#[derive(Debug, Clone, Default)]
pub struct Function {
    pub name: String,
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
#[derive(Debug, Clone)]
pub struct CompiledUnit {
    pub main: Function,
    pub functions: Vec<Function>,
    pub hoisted: Vec<usize>,
    pub file: String,
}
impl Instruction {
    /// Visits all register operands, including writable argument paths.
    pub fn map_registers(&mut self, mut map: impl FnMut(Reg) -> Reg) {
        fn place(p: &mut Place, m: &mut impl FnMut(Reg) -> Reg) {
            p.base = m(p.base);
            for r in p.path.iter_mut().flatten() {
                *r = m(*r)
            }
        }
        match self {
            Instruction::DefaultCheck { param, .. } => *param = map(*param),
            Instruction::StaticCheck { local, .. } => *local = map(*local),
            Instruction::StaticInit { local, src, .. } => {
                *local = map(*local);
                *src = map(*src);
            }
            Instruction::Check { local, .. } => *local = map(*local),
            Instruction::DynamicGlobal { name } => *name = map(*name),
            Instruction::Clear { register } => *register = map(*register),
            Instruction::StartCall { .. } => {}
            Instruction::StartDynamicCall { callee } => *callee = map(*callee),
            Instruction::FinishCall { dst } | Instruction::FinishCallable { dst } => {
                *dst = map(*dst)
            }
            Instruction::Send { argument } => {
                argument.value = map(argument.value);
                if let Some(target) = &mut argument.place {
                    place(target, &mut map);
                }
            }
            Instruction::IncPlace {
                dst, place: target, ..
            } => {
                *dst = map(*dst);
                place(target, &mut map);
            }
            Instruction::Compound {
                dst,
                place: target,
                source,
                ..
            } => {
                *dst = map(*dst);
                *source = map(*source);
                place(target, &mut map);
            }

            Instruction::Load { dst, .. }
            | Instruction::Constant { dst, .. }
            | Instruction::Array(dst)
            | Instruction::Echo(dst)
            | Instruction::ReturnReference(dst)
            | Instruction::Return(dst)
            | Instruction::Exit(dst) => *dst = map(*dst),
            Instruction::Include { dst, path: src, .. }
            | Instruction::Move { dst, src }
            | Instruction::Unary { dst, src, .. } => {
                *dst = map(*dst);
                *src = map(*src)
            }
            Instruction::Reference { dst, place: p } | Instruction::Read { dst, place: p, .. } => {
                *dst = map(*dst);
                place(p, &mut map)
            }
            Instruction::Rebind { place: p, src } | Instruction::Write { place: p, src } => {
                place(p, &mut map);
                *src = map(*src)
            }
            Instruction::Bind { place: p, source } => {
                place(p, &mut map);
                place(source, &mut map)
            }
            Instruction::Unset(p) => place(p, &mut map),
            Instruction::Binary { dst, lhs, rhs, .. } => {
                *dst = map(*dst);
                *lhs = map(*lhs);
                *rhs = map(*rhs)
            }
            Instruction::Branch { value, .. }
            | Instruction::BranchNull { value, .. }
            | Instruction::Define { src: value, .. }
            | Instruction::Global { local: value, .. } => *value = map(*value),
            Instruction::Call {
                dst,
                name,
                arguments,
                ..
            } => {
                *dst = map(*dst);
                *name = map(*name);
                for a in arguments {
                    a.value = map(a.value);
                    if let Some(p) = &mut a.place {
                        place(p, &mut map)
                    }
                }
            }
            Instruction::ArrayPush {
                array, key, value, ..
            } => {
                *array = map(*array);
                *value = map(*value);
                if let Some(k) = key {
                    *k = map(*k)
                }
            }
            Instruction::IterInit { id, src, place: p } => {
                *id = map(*id);
                *src = map(*src);
                if let Some(p) = p {
                    place(p, &mut map)
                }
            }
            Instruction::IterNext { id, key, value, .. } => {
                *id = map(*id);
                *key = map(*key);
                *value = map(*value)
            }
            Instruction::MatchError { subject } => *subject = map(*subject),
            Instruction::Jump(_) | Instruction::Suppress(_) | Instruction::Declare(_) => {}
        }
    }
}
