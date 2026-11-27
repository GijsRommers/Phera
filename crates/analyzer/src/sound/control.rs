//! Where control can enter the middle of a function: `if ($ok) { ... }`
//! jumps past its body, and a `catch` starts where the handler table says.

use phera_bytecode::{Function, Instruction, PathId, Target};

/// The instruction this one can jump to. No instruction has two targets.
///
/// Every instruction is named, so that adding one to the bytecode makes this
/// function fail to compile until the new instruction is placed.
pub(crate) fn branch_target(instruction: &Instruction) -> Option<Target> {
    match *instruction {
        Instruction::FinallyCall { target, .. }
        | Instruction::InstanceOfBranch { target, .. }
        | Instruction::InstanceOfBranchIf { target, .. }
        | Instruction::BinaryIntBranch { target, .. }
        | Instruction::BinaryIntBranchIf { target, .. }
        | Instruction::BinaryBranch { target, .. }
        | Instruction::BinaryBranchIf { target, .. }
        | Instruction::Jump { target }
        | Instruction::JumpIf { target, .. }
        | Instruction::JumpIfNot { target, .. }
        | Instruction::JumpIfNull { target, .. }
        | Instruction::JumpIfNotNull { target, .. }
        | Instruction::DynamicInstanceOfBranch { target, .. }
        | Instruction::DynamicInstanceOfBranchIf { target, .. } => Some(target),
        Instruction::BindStatic { initialized, .. } => Some(initialized),
        Instruction::IterStart { end, .. }
        | Instruction::IterStartReference { end, .. }
        | Instruction::IterNext { end, .. }
        | Instruction::IterNextKeyed { end, .. } => Some(end),
        Instruction::ReceiveDefault { passed, .. } => Some(passed),
        Instruction::GuardBuiltin { call, .. } => Some(call),
        Instruction::GeneratorStart
        | Instruction::Yield { .. }
        | Instruction::YieldFrom { .. }
        | Instruction::GeneratorReturn { .. }
        | Instruction::AttributeDiagnostics
        | Instruction::CheckReturn { .. }
        | Instruction::ReturnChecked { .. }
        | Instruction::FinallyReturn { .. }
        | Instruction::FinallyDiscard { .. }
        | Instruction::Throw { .. }
        | Instruction::FetchClassConstant { .. }
        | Instruction::FetchNamedClassConstant { .. }
        | Instruction::Nop
        | Instruction::DeclareClass { .. }
        | Instruction::New { .. }
        | Instruction::InitMethodCall { .. }
        | Instruction::FetchClassName { .. }
        | Instruction::FetchThis { .. }
        | Instruction::Clone { .. }
        | Instruction::InstanceOf { .. }
        | Instruction::LoadConst { .. }
        | Instruction::LoadNull { .. }
        | Instruction::LoadBool { .. }
        | Instruction::LoadInt { .. }
        | Instruction::Move { .. }
        | Instruction::Clear { .. }
        | Instruction::Assign { .. }
        | Instruction::AssignOp { .. }
        | Instruction::IncDecLocal { .. }
        | Instruction::UnsetLocal { .. }
        | Instruction::IssetLocal { .. }
        | Instruction::Fetch { .. }
        | Instruction::AppendLocal { .. }
        | Instruction::AssignPath { .. }
        | Instruction::AssignPathFromLocal { .. }
        | Instruction::AssignOpPath { .. }
        | Instruction::IncDecPath { .. }
        | Instruction::UnsetPath { .. }
        | Instruction::IssetPath { .. }
        | Instruction::MakeReference { .. }
        | Instruction::BindReference { .. }
        | Instruction::BindDynamicGlobal { .. }
        | Instruction::BindGlobal { .. }
        | Instruction::InitStatic { .. }
        | Instruction::BinaryInt { .. }
        | Instruction::Binary { .. }
        | Instruction::Unary { .. }
        | Instruction::Cast { .. }
        | Instruction::NewArray { .. }
        | Instruction::ArrayPush { .. }
        | Instruction::ArraySet { .. }
        | Instruction::ArraySpread { .. }
        | Instruction::ArrayPushReference { .. }
        | Instruction::ArraySetReference { .. }
        | Instruction::IterEnd { .. }
        | Instruction::InitCall { .. }
        | Instruction::InitDynamicCall { .. }
        | Instruction::SendValue { .. }
        | Instruction::SendLocal { .. }
        | Instruction::SendPath { .. }
        | Instruction::SendSpread { .. }
        | Instruction::NameArgument { .. }
        | Instruction::Call { .. }
        | Instruction::AssignTake { .. }
        | Instruction::SendTake { .. }
        | Instruction::AssignConst { .. }
        | Instruction::SendConst { .. }
        | Instruction::CallMethod { .. }
        | Instruction::CallDirect { .. }
        | Instruction::MakeCallable { .. }
        | Instruction::CheckType { .. }
        | Instruction::ReturnReference { .. }
        | Instruction::Return { .. }
        | Instruction::ReturnNull
        | Instruction::ReturnMissing
        | Instruction::DeclareFunction { .. }
        | Instruction::FetchConstant { .. }
        | Instruction::DeclareConstant { .. }
        | Instruction::Echo { .. }
        | Instruction::Include { .. }
        | Instruction::Exit { .. }
        | Instruction::BeginSilence
        | Instruction::EndSilence
        | Instruction::Raise { .. }
        | Instruction::NewDynamic { .. }
        | Instruction::DynamicMethodCall { .. }
        | Instruction::StaticCall { .. }
        | Instruction::DynamicInstanceOf { .. }
        | Instruction::InitStaticCall { .. }
        | Instruction::FetchIndex { .. }
        | Instruction::ThisOrNull { .. }
        | Instruction::Capture { .. }
        | Instruction::MakeClosure { .. }
        | Instruction::FetchDynamicClassConstant { .. }
        | Instruction::Eval { .. }
        | Instruction::BuiltinUnary { .. }
        | Instruction::NamespacedBuiltin { .. }
        | Instruction::CallNativeDirect { .. }
        | Instruction::BuiltinFrame { .. }
        | Instruction::ArrayKeyExists { .. }
        | Instruction::InLiteralArray { .. }
        | Instruction::InitUserCallback { .. }
        | Instruction::SendUserCallback { .. } => None,
    }
}

/// One flag per instruction: whether a jump, an exception handler or the
/// return of a method site can arrive there from somewhere other than the
/// instruction before it.
pub(crate) fn jump_entries(function: &Function) -> Vec<bool> {
    let mut entries = vec![false; function.instructions.len()];
    let mut mark = |target: Target| {
        if let Some(entry) = entries.get_mut(target.index()) {
            *entry = true;
        }
    };
    for instruction in &function.instructions {
        if let Some(target) = branch_target(instruction) {
            mark(target);
        }
    }
    for handler in &function.handlers {
        mark(handler.start);
        mark(handler.end);
        if let Some(catch) = handler.catch {
            mark(catch);
        }
        if let Some(finally) = handler.finally {
            mark(finally);
        }
    }
    for site in &function.method_sites {
        mark(site.after);
    }
    entries
}

/// The access path an instruction walks, such as `$a['x']` of `$a['x'] = 1`.
pub(crate) fn path_of(instruction: &Instruction) -> Option<PathId> {
    match *instruction {
        Instruction::Fetch { path, .. }
        | Instruction::AssignPath { path, .. }
        | Instruction::AssignPathFromLocal { path, .. }
        | Instruction::AssignOpPath { path, .. }
        | Instruction::IncDecPath { path, .. }
        | Instruction::UnsetPath { path }
        | Instruction::IssetPath { path, .. }
        | Instruction::MakeReference { path, .. }
        | Instruction::BindReference { path, .. }
        | Instruction::IterStartReference { path, .. }
        | Instruction::SendPath { path }
        | Instruction::FetchIndex { path, .. } => Some(path),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::jump_entries;
    use phera_bytecode::{Function, Handler, Instruction, Reg, Target};

    #[test]
    fn jumps_and_handlers_mark_where_control_can_arrive() {
        let function = Function {
            instructions: vec![
                Instruction::JumpIfNot {
                    condition: Reg(0),
                    target: Target(3),
                },
                Instruction::Nop,
                Instruction::Jump { target: Target(4) },
                Instruction::Nop,
                Instruction::Nop,
                Instruction::ReturnNull,
            ]
            .into(),
            handlers: vec![Handler {
                start: Target(1),
                end: Target(2),
                catch: Some(Target(5)),
                finally: None,
                exception: Reg(1),
                cleanup: Vec::new(),
            }],
            ..Function::default()
        };
        assert_eq!(
            jump_entries(&function),
            [false, true, true, true, true, true]
        );
    }

    #[test]
    fn a_target_past_the_end_is_ignored() {
        let function = Function {
            instructions: vec![Instruction::Jump { target: Target(9) }].into(),
            ..Function::default()
        };
        assert_eq!(jump_entries(&function), [false]);
    }
}
