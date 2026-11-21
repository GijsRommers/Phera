//! The registers an instruction writes and reads, for the local-path-key pass.
//!
//! Paths add their base and every evaluated key as reads and their root or
//! scratch register as a write (a write walks the root in place). A direct
//! call's argument area is both read and written (by-reference arguments are
//! written back). The catch-all is deliberately conservative: an instruction
//! not listed here may read and may write every register.

use phera_bytecode::{Function, Instruction, PathBase, PathId, Reg, Step};

/// What one instruction does to registers.
#[derive(Debug, Default)]
pub(crate) struct Operands {
    pub(crate) writes: Vec<Reg>,
    pub(crate) reads: Vec<Reg>,
    /// Set for instructions the table does not list: anything may happen.
    pub(crate) unknown: bool,
}

impl Operands {
    fn new(writes: Vec<Reg>, reads: Vec<Reg>) -> Self {
        Self {
            writes,
            reads,
            unknown: false,
        }
    }

    pub(crate) fn reads(&self, register: Reg) -> bool {
        self.unknown || self.reads.contains(&register)
    }

    pub(crate) fn writes(&self, register: Reg) -> bool {
        self.unknown || self.writes.contains(&register)
    }
}

/// The path operand of an instruction that walks one, if any.
pub(crate) fn path_operand(instruction: Instruction) -> Option<PathId> {
    match instruction {
        Instruction::Fetch { path, .. }
        | Instruction::FetchIndex { path, .. }
        | Instruction::AssignPath { path, .. }
        | Instruction::AssignPathFromLocal { path, .. }
        | Instruction::AssignOpPath { path, .. }
        | Instruction::IncDecPath { path, .. }
        | Instruction::UnsetPath { path }
        | Instruction::IssetPath { path, .. }
        | Instruction::MakeReference { path, .. }
        | Instruction::BindReference { path, .. }
        | Instruction::SendPath { path }
        | Instruction::IterStartReference { path, .. } => Some(path),
        _ => None,
    }
}

/// The registers `instruction` writes and reads.
pub(crate) fn operand_regs(function: &Function, instruction: Instruction) -> Operands {
    use Instruction as I;
    macro_rules! ops {
        (w [$($w:expr),* $(,)?]; r [$($r:expr),* $(,)?]) => {
            Operands::new(vec![$($w),*], vec![$($r),*])
        };
    }
    let mut operands = match instruction {
        I::Nop
        | I::GeneratorStart
        | I::AttributeDiagnostics
        | I::BeginSilence
        | I::EndSilence
        | I::DeclareClass { .. }
        | I::DeclareFunction { .. }
        | I::New { .. }
        | I::InitCall { .. }
        | I::InitStaticCall { .. }
        | I::NameArgument { .. }
        | I::IterEnd { .. }
        | I::ReturnNull
        | I::ReturnMissing => ops!(w []; r []),
        I::Yield { dst, key, value } => {
            let mut reads = vec![];
            if let Some(key) = key {
                reads.push(key);
            }
            if let Some(value) = value {
                reads.push(value);
            }
            Operands::new(vec![dst], reads)
        }
        I::YieldFrom { dst, src } => ops!(w [dst]; r [src]),
        I::GeneratorReturn { src }
        | I::CheckReturn { source: src }
        | I::ReturnChecked { source: src }
        | I::Throw { source: src }
        | I::Return { src }
        | I::ReturnReference { src }
        | I::Echo { src }
        | I::Exit { src }
        | I::SendValue { src }
        | I::SendSpread { src }
        | I::SendUserCallback { src, .. }
        | I::DeclareConstant { src, .. }
        | I::Raise { detail: src, .. }
        | I::Capture { src, .. } => ops!(w []; r [src]),
        I::FinallyCall { state, .. } | I::FinallyReturn { state } | I::FinallyDiscard { state } => {
            ops!(w [state]; r [state])
        }
        I::FetchClassConstant { dst, class, .. } => ops!(w [dst]; r [class]),
        I::FetchNamedClassConstant { dst, .. }
        | I::FetchClassName { dst, .. }
        | I::FetchThis { dst }
        | I::LoadConst { dst, .. }
        | I::LoadNull { dst }
        | I::LoadBool { dst, .. }
        | I::LoadInt { dst, .. }
        | I::NewArray { dst, .. }
        | I::Call { dst, .. }
        | I::MakeCallable { dst }
        | I::FetchConstant { dst, .. }
        | I::ThisOrNull { dst }
        | I::MakeClosure { dst, .. }
        | I::BuiltinFrame { dst, .. } => ops!(w [dst]; r []),
        I::Clone { dst, src, with } => {
            let mut reads = vec![src];
            if let Some(with) = with {
                reads.push(with);
            }
            Operands::new(vec![dst], reads)
        }
        I::InstanceOf { dst, src, .. } | I::BuiltinUnary { dst, src, .. } => ops!(w [dst]; r [src]),
        I::Move { dst, src } => ops!(w [dst]; r [src]),
        I::Clear { reg } | I::UnsetLocal { local: reg } => ops!(w [reg]; r []),
        I::Assign { local, src } => ops!(w [local]; r [src]),
        I::AssignOp { local, src, .. } | I::AppendLocal { local, src } => {
            ops!(w [local]; r [local, src])
        }
        I::IncDecLocal { dst, local, .. } => ops!(w [dst, local]; r [local]),
        I::IssetLocal { dst, local } => ops!(w [dst]; r [local]),
        I::CheckType { local, .. } | I::BindStatic { local, .. } => ops!(w [local]; r [local]),
        I::BindGlobal { local, .. } | I::ReceiveDefault { param: local, .. } => {
            ops!(w [local]; r [])
        }
        I::InitStatic { local, src, .. } => ops!(w [local]; r [src]),
        I::Fetch { dst, .. } | I::IssetPath { dst, .. } => ops!(w [dst]; r []),
        I::AssignPathFromLocal { dst, src, .. } | I::AssignOpPath { dst, src, .. } => {
            ops!(w [dst]; r [src])
        }
        I::IncDecPath { dst, .. } => ops!(w [dst]; r []),
        I::UnsetPath { .. } | I::AssignPath { .. } | I::SendPath { .. } => ops!(w []; r []),
        I::MakeReference { dst, .. } => ops!(w [dst]; r []),
        I::BindReference { src, .. } => ops!(w []; r [src]),
        I::BindDynamicGlobal { name } => ops!(w []; r [name]),
        I::Include { dst, path, .. } => ops!(w [dst]; r [path]),
        I::BinaryIntBranch { lhs, .. } => ops!(w []; r [lhs]),
        I::BinaryInt { dst, lhs, .. } => ops!(w [dst]; r [lhs]),
        I::Binary { dst, lhs, rhs, .. } => ops!(w [dst]; r [lhs, rhs]),
        I::Unary { dst, src, .. } | I::Cast { dst, src, .. } => ops!(w [dst]; r [src]),
        I::ArrayPush { array, src }
        | I::ArraySpread { array, src }
        | I::ArrayPushReference { array, src } => ops!(w [array]; r [array, src]),
        I::ArraySet { array, key, src } | I::ArraySetReference { array, key, src } => {
            ops!(w [array]; r [array, key, src])
        }
        I::Jump { .. } => ops!(w []; r []),
        I::JumpIf { condition, .. }
        | I::JumpIfNot { condition, .. }
        | I::JumpIfNull {
            value: condition, ..
        }
        | I::JumpIfNotNull {
            value: condition, ..
        } => ops!(w []; r [condition]),
        I::IterStart { src, .. } => ops!(w [src]; r [src]),
        I::IterStartReference { subject, .. } => ops!(w [subject]; r []),
        I::IterNext { value, .. } => ops!(w [value]; r []),
        I::IterNextKeyed { key, value, .. } => ops!(w [key, value]; r []),
        I::InitDynamicCall { callee }
        | I::InitUserCallback {
            callback: callee, ..
        } => {
            ops!(w []; r [callee])
        }
        I::InitMethodCall { receiver, .. } => ops!(w []; r [receiver]),
        I::DynamicMethodCall {
            receiver, method, ..
        }
        | I::StaticCall {
            class: receiver,
            method,
            ..
        } => ops!(w []; r [receiver, method]),
        I::NewDynamic { class } => ops!(w []; r [class]),
        I::DynamicInstanceOf { dst, source, class } => ops!(w [dst]; r [source, class]),
        I::FetchIndex {
            dst, base, index, ..
        } => ops!(w [dst]; r [base, index]),
        I::FetchDynamicClassConstant { dst, class, name } => ops!(w [dst]; r [class, name]),
        I::Eval { dst, source } => ops!(w [dst]; r [source]),
        I::CallDirect {
            args, count, dst, ..
        }
        | I::CallNativeDirect {
            args, count, dst, ..
        } => {
            let area: Vec<Reg> = (0..count).map(|offset| Reg(args.0 + offset)).collect();
            let mut writes = vec![dst];
            writes.extend(area.iter().copied());
            Operands::new(writes, area)
        }
        I::ArrayKeyExists { dst, key, array } => ops!(w [dst]; r [key, array]),
        I::InLiteralArray { dst, needle, .. } => ops!(w [dst]; r [needle]),
        _ => Operands {
            unknown: true,
            ..Default::default()
        },
    };
    if let Some(path_id) = path_operand(instruction)
        && let Some(path) = function.paths.get(path_id.index())
    {
        match path.base {
            PathBase::Register(root) | PathBase::Named(root) => {
                operands.writes.push(root);
                operands.reads.push(root);
            }
            PathBase::StaticProperty {
                class,
                name,
                scratch,
            } => {
                operands.reads.push(class);
                operands.reads.push(name);
                operands.writes.push(scratch);
            }
            PathBase::NamedStaticProperty { scratch, .. } | PathBase::This { scratch } => {
                operands.writes.push(scratch);
            }
            PathBase::GlobalNamed { name, scratch } => {
                operands.reads.push(name);
                operands.writes.push(scratch);
            }
            PathBase::Globals => {}
        }
        for step in &path.steps {
            match step {
                Step::Index(key) | Step::DynamicProperty(key) => operands.reads.push(*key),
                Step::Property(_) | Step::Append => {}
            }
        }
    }
    operands
}

/// Whether an instruction can run PHP code behind the compiler's back: a
/// call, an include or eval, a magic property or dimension, an operator that
/// can call `__toString`, a clone or an echo.
pub(crate) fn runs_php_code(instruction: Instruction) -> bool {
    use Instruction as I;
    match instruction {
        I::InitCall { .. }
        | I::InitDynamicCall { .. }
        | I::SendValue { .. }
        | I::SendLocal { .. }
        | I::SendSpread { .. }
        | I::NameArgument { .. }
        | I::Call { .. }
        | I::CallDirect { .. }
        | I::CallNativeDirect { .. }
        | I::MakeCallable { .. }
        | I::InitUserCallback { .. }
        | I::SendUserCallback { .. }
        | I::InitMethodCall { .. }
        | I::DynamicMethodCall { .. }
        | I::StaticCall { .. }
        | I::InitStaticCall { .. }
        | I::New { .. }
        | I::NewDynamic { .. }
        | I::Include { .. }
        | I::Eval { .. }
        | I::Clone { .. }
        | I::Echo { .. }
        | I::YieldFrom { .. }
        | I::IterStart { .. }
        | I::IterStartReference { .. }
        | I::IterNext { .. }
        | I::IterNextKeyed { .. }
        | I::FetchClassConstant { .. }
        | I::FetchNamedClassConstant { .. }
        | I::FetchConstant { .. }
        | I::FetchDynamicClassConstant { .. }
        | I::DeclareClass { .. }
        | I::AttributeDiagnostics
        | I::Fetch { .. }
        | I::FetchIndex { .. }
        | I::AssignPath { .. }
        | I::AssignPathFromLocal { .. }
        | I::AssignOpPath { .. }
        | I::IncDecPath { .. }
        | I::UnsetPath { .. }
        | I::IssetPath { .. }
        | I::MakeReference { .. }
        | I::BindReference { .. }
        | I::SendPath { .. } => true,
        I::Binary { op, .. } => op == phera_bytecode::BinaryOp::Concat,
        I::Cast { to, .. } => to == phera_bytecode::CastKind::String,
        _ => false,
    }
}
