//! Register reads and possible writes of emitted bytecode.

use phera_bytecode::{Function, Instruction, PathBase, PathId, Reg, Step};

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
pub(super) fn visit(
    function: &Function,
    instruction: Instruction,
    mut read: impl FnMut(Reg),
    mut write: impl FnMut(Reg),
) -> bool {
    use Instruction as I;
    macro_rules! ops {
        (w [$($w:expr),* $(,)?]; r [$($r:expr),* $(,)?]) => {
            { $(write($w);)* $(read($r);)* }
        };
    }
    match instruction {
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
            write(dst);
            key.into_iter().chain(value).for_each(&mut read);
        }
        I::YieldFrom { dst, src } => ops!(w [dst]; r [src]),
        I::GeneratorReturn { src }
        | I::ReturnChecked { source: src }
        | I::Throw { source: src }
        | I::Return { src }
        | I::ReturnReference { src }
        | I::Echo { src }
        | I::Exit { src }
        | I::SendValue { src, .. }
        | I::SendSpread { src }
        | I::SendUserCallback { src, .. }
        | I::DeclareConstant { src, .. }
        | I::Raise { detail: src, .. } => ops!(w []; r [src]),
        I::Capture { src, .. } | I::CheckReturn { source: src } => ops!(w [src]; r [src]),
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
            write(dst);
            read(src);
            with.into_iter().for_each(&mut read);
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
        I::BindReference { src, .. } => ops!(w [src]; r [src]),
        // On a miss this reads the local and writes the following call's area.
        I::CallDirectLocal { local, area } => ops!(w [area]; r [local]),
        I::SendLocal { local } => ops!(w [local]; r [local]),
        I::BindDynamicGlobal { name } => ops!(w []; r [name]),
        I::Include { dst, path, .. } => ops!(w [dst]; r [path]),
        I::BinaryIntBranch { lhs, .. } | I::BinaryIntBranchIf { lhs, .. } => ops!(w []; r [lhs]),
        I::BinaryBranch { lhs, rhs, .. } | I::BinaryBranchIf { lhs, rhs, .. } => {
            ops!(w []; r [lhs, rhs])
        }
        I::BinaryInt { dst, lhs, .. } => ops!(w [dst]; r [lhs]),
        I::Binary { dst, lhs, rhs, .. } => ops!(w [dst]; r [lhs, rhs]),
        I::Unary { dst, src, .. } | I::Cast { dst, src, .. } => ops!(w [dst]; r [src]),
        I::ArrayPush { array, src } | I::ArraySpread { array, src } => {
            ops!(w [array]; r [array, src])
        }
        I::ArrayPushReference { array, src } => ops!(w [array, src]; r [array, src]),
        I::ArraySet { array, key, src } => ops!(w [array]; r [array, key, src]),
        I::ArraySetReference { array, key, src } => ops!(w [array, src]; r [array, key, src]),
        I::Jump { .. } => ops!(w []; r []),
        I::JumpIf { condition, .. }
        | I::JumpIfNot { condition, .. }
        | I::JumpIfNull {
            value: condition, ..
        }
        | I::JumpIfNotNull {
            value: condition, ..
        }
        | I::InstanceOfBranch { src: condition, .. }
        | I::InstanceOfBranchIf { src: condition, .. } => ops!(w []; r [condition]),
        I::DynamicInstanceOfBranch { src, class, .. }
        | I::DynamicInstanceOfBranchIf { src, class, .. } => ops!(w []; r [src, class]),
        I::GuardBuiltin { .. } => ops!(w []; r []),
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
            write(dst);
            for offset in 0..count {
                let register = Reg(args.0 + offset);
                read(register);
                write(register);
            }
        }
        I::ArrayKeyExists { dst, key, array } => ops!(w [dst]; r [key, array]),
        I::InLiteralArray { dst, needle, .. } => ops!(w [dst]; r [needle]),
        _ => return false,
    }
    if let Some(path_id) = path_operand(instruction)
        && let Some(path) = function.paths.get(path_id.index())
    {
        match path.base {
            PathBase::Register(root) | PathBase::Named(root) => {
                write(root);
                read(root);
            }
            PathBase::StaticProperty {
                class,
                name,
                scratch,
            } => {
                read(class);
                read(name);
                write(scratch);
            }
            PathBase::NamedStaticProperty { scratch, .. } | PathBase::This { scratch } => {
                write(scratch);
            }
            PathBase::GlobalNamed { name, scratch } => {
                read(name);
                write(scratch);
            }
            PathBase::Globals => {}
        }
        for step in &path.steps {
            match step {
                Step::Index(key) | Step::DynamicProperty(key) => read(*key),
                Step::Property(_) | Step::Append => {}
            }
        }
    }
    true
}
