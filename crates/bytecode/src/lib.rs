//! Immutable, shareable register bytecode. PHP values exist only in the VM.
use std::fmt;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reg(pub u16);
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
    Callable {
        dst: Reg,
        name: Reg,
        fallback: Option<String>,
    },
    ReadArgument {
        dst: Reg,
        place: Place,
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
    Increment {
        dst: Reg,
        src: Reg,
        increment: bool,
    },
    Static {
        local: Reg,
        name: String,
        default: Constant,
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
        op: Binary,
        lhs: Reg,
        rhs: Reg,
    },
    Unary {
        dst: Reg,
        op: Unary,
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
    Fatal {
        class: String,
        message: String,
    },
}
#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub default: Option<Constant>,
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
    /// Reserved for catch/finally ranges; all throws use the same frame unwind path.
    pub handlers: Vec<Handler>,
}
#[derive(Debug, Clone)]
pub struct Handler {
    pub start: Target,
    pub end: Target,
    pub target: Target,
}
#[derive(Debug, Clone)]
pub struct CompiledUnit {
    pub main: Function,
    pub functions: Vec<Function>,
    pub hoisted: Vec<usize>,
    pub constants: Vec<(String, Constant)>,
    pub file: String,
}
const _: fn() = || {
    fn check<T: Send + Sync>() {}
    check::<CompiledUnit>();
};
impl fmt::Display for CompiledUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "file {}", self.file)?;
        for fun in std::iter::once(&self.main).chain(&self.functions) {
            writeln!(f, "function {} ({} registers)", fun.name, fun.registers)?;
            for (i, op) in fun.instructions.iter().enumerate() {
                writeln!(
                    f,
                    "  {i:05} L{} {op:?}",
                    fun.lines.get(i).copied().unwrap_or(1)
                )?;
            }
        }
        Ok(())
    }
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
        use Instruction::*;
        match self {
            Callable { dst, name, .. } => {
                *dst = map(*dst);
                *name = map(*name)
            }
            Load { dst, .. }
            | Constant { dst, .. }
            | Array(dst)
            | Echo(dst)
            | Return(dst)
            | Exit(dst) => *dst = map(*dst),
            Include { dst, path: src, .. }
            | Increment { dst, src, .. }
            | Move { dst, src }
            | Unary { dst, src, .. } => {
                *dst = map(*dst);
                *src = map(*src)
            }
            ReadArgument { dst, place: p }
            | Reference { dst, place: p }
            | Read { dst, place: p, .. } => {
                *dst = map(*dst);
                place(p, &mut map)
            }
            Rebind { place: p, src } | Write { place: p, src } => {
                place(p, &mut map);
                *src = map(*src)
            }
            Bind { place: p, source } => {
                place(p, &mut map);
                place(source, &mut map)
            }
            Unset(p) => place(p, &mut map),
            Binary { dst, lhs, rhs, .. } => {
                *dst = map(*dst);
                *lhs = map(*lhs);
                *rhs = map(*rhs)
            }
            Branch { value, .. }
            | BranchNull { value, .. }
            | Define { src: value, .. }
            | Global { local: value, .. }
            | Static { local: value, .. } => *value = map(*value),
            Call {
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
            ArrayPush {
                array, key, value, ..
            } => {
                *array = map(*array);
                *value = map(*value);
                if let Some(k) = key {
                    *k = map(*k)
                }
            }
            IterInit { id, src, place: p } => {
                *id = map(*id);
                *src = map(*src);
                if let Some(p) = p {
                    place(p, &mut map)
                }
            }
            IterNext { id, key, value, .. } => {
                *id = map(*id);
                *key = map(*key);
                *value = map(*value)
            }
            Jump(_) | Suppress(_) | Declare(_) | Fatal { .. } => {}
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disassembly_is_readable() {
        let unit = CompiledUnit {
            main: Function {
                name: "{main}".into(),
                instructions: vec![Instruction::Echo(Reg(0))],
                lines: vec![7],
                registers: 1,
                ..Function::default()
            },
            functions: vec![],
            hoisted: vec![],
            constants: vec![],
            file: "hello.php".into(),
        };
        let dump = unit.to_string();
        assert!(dump.contains("L7 Echo(Reg(0))"));
        assert!(dump.contains("function {main} (1 registers)"));
    }
}
