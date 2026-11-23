//! Remove scalar temporary clears and dead statement results within basic blocks.
//!
//! Heap values must still be cleared promptly for copy-on-write. Facts are reset
//! at every control-flow boundary, so no branch-dependent type is assumed.
//!
//! Input is emitted bytecode with valid register and target indices. Output
//! retains evaluation order and remaps all targets after removed instructions.
//! For example, LoadInt followed by Binary can become BinaryInt when the
//! literal temporary is proven dead; scalar-only Clear instructions disappear.

use std::collections::HashSet;

use phera_bytecode::{BinaryOp, Function, Instruction, PathBase, Reg, Target};

use crate::passes::integer_operands::fuse_integer_operands;

/// Optimize operands in place while retaining source locations and valid branch targets.
pub(crate) fn optimize(function: &mut Function) {
    // Exception edges carry live temporaries not described by ordinary branch operands.
    // Preserve their layout until the optimizer models exceptional successors.
    if !function.handlers.is_empty() {
        return;
    }
    let boundaries: HashSet<_> = function
        .instructions
        .iter()
        .filter_map(target)
        .map(|target| target.index())
        .collect();
    let mut facts = vec![Fact::Unknown; function.registers as usize];
    let mut offsets = Vec::new();
    let mut instructions = Vec::new();
    let mut lines = Vec::new();
    for (index, instruction) in function.instructions.iter().copied().enumerate() {
        offsets.push(instructions.len() as u32);
        if boundaries.contains(&index) {
            facts.fill(Fact::Unknown);
        }
        let remove = match instruction {
            Instruction::Nop => true,
            // A named local is never a temporary to drop early.
            Instruction::Clear { reg } => {
                reg.index() >= function.locals.len() && fact(&facts, reg) != Fact::Unknown
            }
            Instruction::Move { dst, .. } => function
                .instructions
                .iter()
                .skip(index + 1)
                .take_while(|instruction| matches!(instruction, Instruction::Clear { .. }))
                .any(|instruction| matches!(instruction,Instruction::Clear { reg } if *reg == dst)),
            _ => false,
        };
        update_facts(function, &mut facts, instruction);
        if !remove {
            instructions.push(instruction);
            lines.push(function.lines.get(index).copied().unwrap_or(1));
        }
    }
    offsets.push(instructions.len() as u32);
    for instruction in &mut instructions {
        if let Some(target) = target_mut(instruction) {
            target.0 = offsets.get(target.index()).copied().unwrap_or(target.0);
        }
    }
    function.instructions = instructions.into();
    function.lines = lines;
    fuse_integer_operands(function);
    crate::passes::direct_operands::optimize(function);
    crate::passes::direct_operands::fold_operand_copies(function);
    crate::passes::integer_operands::fuse_integer_branches(function);
    crate::passes::direct_operands::specialize_appends(function);
    crate::passes::direct_operands::specialize_indexes(function);
    compact_nops(function);
}

/// Read the target of a control-flow instruction when it has one.
pub(crate) fn target(instruction: &Instruction) -> Option<Target> {
    let mut instruction = *instruction;
    target_mut(&mut instruction).copied()
}

/// Expose the target operand for offset remapping after instruction removal.
pub(crate) fn target_mut(instruction: &mut Instruction) -> Option<&mut Target> {
    match instruction {
        Instruction::BinaryIntBranch { target, .. }
        | Instruction::BinaryIntBranchIf { target, .. }
        | Instruction::BinaryBranch { target, .. }
        | Instruction::BinaryBranchIf { target, .. }
        | Instruction::InstanceOfBranch { target, .. }
        | Instruction::InstanceOfBranchIf { target, .. }
        | Instruction::DynamicInstanceOfBranch { target, .. }
        | Instruction::DynamicInstanceOfBranchIf { target, .. }
        | Instruction::Jump { target }
        | Instruction::JumpIf { target, .. }
        | Instruction::JumpIfNot { target, .. }
        | Instruction::JumpIfNull { target, .. }
        | Instruction::JumpIfNotNull { target, .. }
        | Instruction::GuardBuiltin { call: target, .. } => Some(target),
        Instruction::ReceiveDefault { passed, .. } => Some(passed),
        Instruction::BindStatic { initialized, .. } => Some(initialized),
        Instruction::FinallyCall { target, .. } => Some(target),
        Instruction::IterNext { end, .. } | Instruction::IterNextKeyed { end, .. } => Some(end),
        _ => None,
    }
}

/// Preserve source locations and targets after integer fusion removes loads.
pub(crate) fn compact_nops(function: &mut Function) {
    let mut offsets = Vec::with_capacity(function.instructions.len() + 1);
    let mut count = 0;
    for instruction in &function.instructions {
        offsets.push(count);
        if !matches!(instruction, Instruction::Nop) {
            count += 1
        }
    }
    offsets.push(count);
    if count as usize == function.instructions.len() {
        return;
    }
    for handler in &mut function.handlers {
        handler.start.0 = offsets[handler.start.index()];
        handler.end.0 = offsets[handler.end.index()];
        for target in [&mut handler.catch, &mut handler.finally]
            .into_iter()
            .flatten()
        {
            target.0 = offsets[target.index()];
        }
    }
    let mut lines = Vec::new();
    let mut instructions = Vec::new();
    for (index, mut instruction) in function.instructions.iter().copied().enumerate() {
        if matches!(instruction, Instruction::Nop) {
            continue;
        }
        if let Some(target) = target_mut(&mut instruction) {
            target.0 = offsets.get(target.index()).copied().unwrap_or(target.0);
        }
        instructions.push(instruction);
        lines.push(function.lines.get(index).copied().unwrap_or(1));
    }
    function.instructions = instructions.into();
    function.lines = lines;
}

/// What the pass knows a register holds at a point of a basic block: what
/// makes a `Clear` of it unnecessary, since dropping the value early frees
/// nothing and makes no array or string unique.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fact {
    Unknown,
    /// Nothing, or a value that owns no heap value: null, a boolean, an
    /// integer, a float. (Never an array, so `+` on it is arithmetic.)
    Scalar,
    /// A value the unit's constant table also owns (`LoadConst`): dropping
    /// this copy early leaves an owner, so it changes nothing. It may be an
    /// array.
    Shared,
}

/// The facts after `instruction`.
fn update_facts(function: &Function, facts: &mut [Fact], instruction: Instruction) {
    use Fact::{Scalar, Shared, Unknown};
    match instruction {
        Instruction::LoadInt { dst, .. }
        | Instruction::LoadBool { dst, .. }
        | Instruction::LoadNull { dst } => set_fact(facts, dst, Scalar),
        Instruction::LoadConst { dst, .. } => set_fact(facts, dst, Shared),
        Instruction::BuiltinUnary { op, dst, .. }
        | Instruction::NamespacedBuiltin { op, dst, .. } => {
            facts.fill(Unknown);
            let scalar = !matches!(
                op,
                phera_bytecode::BuiltinUnary::GetType
                    | phera_bytecode::BuiltinUnary::GetClass
                    | phera_bytecode::BuiltinUnary::StrToLower
            );
            set_fact(facts, dst, if scalar { Scalar } else { Unknown });
        }
        Instruction::BuiltinFrame {
            op: phera_bytecode::BuiltinFrame::ArgumentCount,
            dst,
        }
        | Instruction::ArrayKeyExists { dst, .. }
        | Instruction::InLiteralArray { dst, .. } => {
            facts.fill(Unknown);
            set_fact(facts, dst, Scalar);
        }
        Instruction::Move { dst, src } | Instruction::Assign { local: dst, src } => {
            set_fact(facts, dst, fact(facts, src))
        }
        Instruction::Binary { op, dst, lhs, rhs } => {
            let value = arithmetic(op, fact(facts, lhs), fact(facts, rhs));
            set_fact(facts, dst, value)
        }
        Instruction::AssignOp { op, local, src } => {
            let value = arithmetic(op, fact(facts, local), fact(facts, src));
            set_fact(facts, local, value)
        }
        // Cleared, the register holds nothing.
        Instruction::Clear { reg } => set_fact(facts, reg, Scalar),
        // `++` and `--` give a number, null, or for a string a string;
        // never an array or object, which throw instead.
        Instruction::IncDecLocal { dst, local, .. } => {
            forget_written(function, facts, instruction);
            set_fact(facts, local, Unknown);
            set_fact(facts, dst, Scalar);
        }
        Instruction::IncDecPath { dst, .. } => {
            forget_written(function, facts, instruction);
            set_fact(facts, dst, Scalar);
        }
        _ => forget_written(function, facts, instruction),
    }
}

/// What a binary operator gives: a scalar, unless it is `.` (a new
/// string) or `+` of two operands that may both be arrays (a union).
fn arithmetic(operator: BinaryOp, left: Fact, right: Fact) -> Fact {
    if operator != BinaryOp::Concat
        && (operator != BinaryOp::Add || left == Fact::Scalar || right == Fact::Scalar)
    {
        Fact::Scalar
    } else {
        Fact::Unknown
    }
}

/// Forgets what an instruction the pass does not model may have changed:
/// every named local (references, callbacks and dynamic scope reach
/// them), and every temporary the instruction may write. For an
/// instruction that walks a path, those are its destination, the path's
/// root (written in place) and scratch register (which the VM fills while
/// it walks the path), and the source a reference is bound to; the keys
/// are only read. For any other, every temporary that
/// `object_liveness::effect` does not call unrelated.
fn forget_written(function: &Function, facts: &mut [Fact], instruction: Instruction) {
    use crate::passes::object_liveness::{RegisterEffect, effect};
    let locals = function.locals.len();
    let path_writes = path_writes(function, instruction);
    for (index, slot) in facts.iter_mut().enumerate() {
        if *slot == Fact::Unknown {
            continue;
        }
        let kept = index >= locals
            && u16::try_from(index).is_ok_and(|index| {
                let register = phera_bytecode::Reg(index);
                match &path_writes {
                    Some(written) => !written.contains(&Some(register)),
                    None => matches!(
                        effect(function, instruction, register),
                        RegisterEffect::Unrelated
                    ),
                }
            });
        if !kept {
            *slot = Fact::Unknown;
        }
    }
}

/// The registers an instruction that walks a path may write (see
/// [`forget_written`]); `None` for any other instruction.
fn path_writes(function: &Function, instruction: Instruction) -> Option<[Option<Reg>; 4]> {
    let (path, destination, bound) = match instruction {
        Instruction::Fetch { dst, path, .. }
        | Instruction::FetchIndex { dst, path, .. }
        | Instruction::MakeReference { dst, path }
        | Instruction::AssignOpPath { dst, path, .. }
        | Instruction::AssignPathFromLocal { dst, path, .. }
        | Instruction::IncDecPath { dst, path, .. } => (path, Some(dst), None),
        Instruction::AssignPath { path, .. } | Instruction::UnsetPath { path } => {
            (path, None, None)
        }
        Instruction::BindReference { path, src } => (path, None, Some(src)),
        _ => return None,
    };
    let base = function.paths.get(path.index())?.base;
    let (root, scratch) = match base {
        PathBase::Register(root) | PathBase::Named(root) => (Some(root), None),
        PathBase::This { scratch }
        | PathBase::StaticProperty { scratch, .. }
        | PathBase::NamedStaticProperty { scratch, .. }
        | PathBase::GlobalNamed { scratch, .. } => (None, Some(scratch)),
        PathBase::Globals => (None, None),
    };
    Some([destination, root, scratch, bound])
}

/// Missing facts are unknown, so they can never justify removing a clear.
fn fact(facts: &[Fact], register: phera_bytecode::Reg) -> Fact {
    facts
        .get(register.index())
        .copied()
        .unwrap_or(Fact::Unknown)
}

/// Record a fact without unchecked indexing.
fn set_fact(facts: &mut [Fact], register: phera_bytecode::Reg, value: Fact) {
    if let Some(slot) = facts.get_mut(register.index()) {
        *slot = value;
    }
}
