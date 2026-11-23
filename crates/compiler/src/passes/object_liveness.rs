//! Class and property operations cannot observe unrelated compiler-only temporaries.
//!
//! This supplements the scalar liveness proof. It never crosses an operand read,
//! and it is used only for registers beyond the named-local prefix: PHP callbacks
//! and dynamic scope can observe named locals, but cannot name these temporaries.

use phera_bytecode::{Function, Instruction, PathBase, PathId, Reg, Step};

/// A known read/overwrite, an unrelated operation, or an unclassified barrier.
pub(crate) enum RegisterEffect {
    Read,
    Write,
    Unrelated,
    Unknown,
}

/// Classify object operations without assuming anything about PHP-visible locals.
pub(crate) fn effect(
    function: &Function,
    instruction: Instruction,
    register: Reg,
) -> RegisterEffect {
    use RegisterEffect::{Read, Unknown, Unrelated, Write};
    match instruction {
        Instruction::New { .. }
        | Instruction::InitStaticCall { .. }
        | Instruction::DeclareClass { .. }
        | Instruction::IterEnd { .. } => Unrelated,
        Instruction::FetchClassName { dst, .. } | Instruction::FetchThis { dst } => {
            if dst == register { Write } else { Unrelated }
        }
        Instruction::NewDynamic { class } => {
            if class == register {
                Read
            } else {
                Unrelated
            }
        }
        Instruction::InitMethodCall { receiver, .. } => {
            if receiver == register {
                Read
            } else {
                Unrelated
            }
        }
        Instruction::DynamicMethodCall {
            receiver: class,
            method,
        }
        | Instruction::StaticCall { class, method, .. } => {
            if class == register || method == register {
                Read
            } else {
                Unrelated
            }
        }
        Instruction::Clone { dst, src, with } => {
            if with == Some(register) {
                Read
            } else {
                read_write(src, dst, register)
            }
        }
        Instruction::InstanceOf { dst, src, .. }
        | Instruction::BuiltinUnary { dst, src, .. }
        | Instruction::NamespacedBuiltin { dst, src, .. } => read_write(src, dst, register),
        Instruction::Fetch { dst, path, .. }
        | Instruction::FetchIndex { dst, path, .. }
        | Instruction::MakeReference { dst, path } => {
            path_effect(function, path, Some(dst), None, register)
        }
        Instruction::AssignPath { path, src } | Instruction::BindReference { path, src } => {
            path_effect(function, path, None, Some(src), register)
        }
        Instruction::AssignOpPath { dst, path, src, .. }
        | Instruction::AssignPathFromLocal { dst, path, src } => {
            path_effect(function, path, Some(dst), Some(src), register)
        }
        Instruction::IncDecPath { dst, path, .. } => {
            path_effect(function, path, Some(dst), None, register)
        }
        Instruction::UnsetPath { path } => path_effect(function, path, None, None, register),
        Instruction::AppendLocal { local, src } => {
            if local == register || src == register {
                Read
            } else {
                Unrelated
            }
        }
        _ => Unknown,
    }
}

/// Source reads win over writes, including instructions that overwrite their source.
fn read_write(source: Reg, destination: Reg, register: Reg) -> RegisterEffect {
    if source == register {
        RegisterEffect::Read
    } else if destination == register {
        RegisterEffect::Write
    } else {
        RegisterEffect::Unrelated
    }
}

/// A path can read its root, class/name operands, and every evaluated index/name.
fn path_effect(
    function: &Function,
    path: PathId,
    destination: Option<Reg>,
    source: Option<Reg>,
    register: Reg,
) -> RegisterEffect {
    let Some(path) = function.paths.get(path.index()) else {
        return RegisterEffect::Unknown;
    };
    let base_reads = match path.base {
        PathBase::Register(base) | PathBase::Named(base) => base == register,
        PathBase::StaticProperty { class, name, .. } => class == register || name == register,
        PathBase::GlobalNamed { name, .. } => name == register,
        PathBase::This { .. } | PathBase::NamedStaticProperty { .. } | PathBase::Globals => false,
    };
    if source == Some(register) || base_reads || path.steps.iter().any(|step| matches!(step, Step::Index(index) | Step::DynamicProperty(index) if *index == register)) { return RegisterEffect::Read; }
    if destination == Some(register) {
        RegisterEffect::Write
    } else {
        RegisterEffect::Unrelated
    }
}
