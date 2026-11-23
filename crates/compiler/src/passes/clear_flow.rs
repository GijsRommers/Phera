//! Elide temporary cleanup when no reachable value owns a heap allocation.
//!
//! In `try { (int) $value; } finally { finish(); }`, dropping the cast's
//! result does no work. Dropping a temporary array still matters for copy on
//! write, and a temporary object can run a destructor.

use std::{collections::VecDeque, ops::Range};

use phera_bytecode::{
    BinaryOp, BuiltinFrame, BuiltinUnary, CastKind, Constant, Function, Handler, Instruction as I,
    Reg, Type, TypeMask, UnaryOp,
};

use super::{
    operand_regs,
    optimize::{compact_nops, target},
};

/// The value in the register itself. Even a reference to a scalar owns memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Fact {
    Empty,
    Scalar,
    NonObject,
    Value,
    Unknown,
}

impl Fact {
    fn owns_nothing(self) -> bool {
        self <= Self::Scalar
    }
}

struct Edge {
    block: usize,
    taken: bool,
}

struct Block {
    instructions: Range<usize>,
    successors: Vec<Edge>,
    handlers: Vec<usize>,
}

/// Remove only redundant releases. The existing operand folds run before this
/// pass, and method-site formation runs after it.
pub(super) fn elide(function: &mut Function, constants: &[Constant], types: &[Type]) {
    let (clears, cleanups) = candidates(function, constants, types);
    let compact = !clears.is_empty();
    for offset in clears {
        function.instructions[offset] = I::Nop;
    }
    for (handler, removed) in function.handlers.iter_mut().zip(cleanups) {
        handler
            .cleanup
            .retain(|register| !removed.contains(register));
    }
    if compact {
        compact_nops(function);
    }
}

fn candidates(
    function: &Function,
    constants: &[Constant],
    types: &[Type],
) -> (Vec<usize>, Vec<Vec<Reg>>) {
    let cleanup = vec![Vec::new(); function.handlers.len()];
    if !function
        .instructions
        .iter()
        .any(|i| matches!(i, I::Clear { .. }))
        && function
            .handlers
            .iter()
            .all(|handler| handler.cleanup.is_empty())
    {
        return (Vec::new(), cleanup);
    }
    let (blocks, at) = blocks(function);
    if blocks.is_empty() {
        return (Vec::new(), cleanup);
    }
    let live = live_at_entries(function, &blocks, &at);
    let (incoming, unwound) = ownership(function, constants, types, &blocks, &at);
    let locals = function.locals.len();
    let mut clears = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let Some(mut facts) = incoming[index].clone() else {
            continue;
        };
        let mut releases = vec![Fact::Unknown; block.instructions.len()];
        for ip in block.instructions.clone() {
            if let I::Clear { reg } = function.instructions[ip]
                && reg.index() >= locals
            {
                releases[ip - block.instructions.start] = facts[reg.index()];
            }
            transfer(function, constants, &mut facts, function.instructions[ip]);
        }
        let mut live_after = successor_reads(function, block, &at, &live);
        for ip in block.instructions.clone().rev() {
            let instruction = function.instructions[ip];
            if let I::Clear { reg } = instruction
                && redundant(
                    releases[ip - block.instructions.start],
                    live_after[reg.index()],
                )
            {
                clears.push(ip);
            }
            read_before_write(function, instruction, &mut live_after);
            add_handler_reads(function, block, &at, &live, &mut live_after);
        }
    }
    let cleanup = function
        .handlers
        .iter()
        .zip(unwound)
        .map(|(handler, facts)| {
            let Some(facts) = facts else {
                return Vec::new();
            };
            handler
                .cleanup
                .iter()
                .copied()
                .filter(|reg| {
                    reg.index() >= locals
                        && redundant(
                            facts[reg.index()],
                            entries(handler).any(|ip| live[at[ip]][reg.index()]),
                        )
                })
                .collect()
        })
        .collect();
    (clears, cleanup)
}

// An empty slot can remain empty even when it has a reader. A scalar can
// remain in its slot only when the old value is dead.
fn redundant(fact: Fact, live: bool) -> bool {
    fact == Fact::Empty || (fact == Fact::Scalar && !live)
}

/// A block ends at a branch or at the boundary of an exception range.
fn blocks(function: &Function) -> (Vec<Block>, Vec<usize>) {
    let instructions = &function.instructions;
    let mut starts = vec![false; instructions.len() + 1];
    starts[0] = true;
    for (ip, instruction) in instructions.iter().enumerate() {
        if let Some(target) = target(instruction) {
            starts[target.index()] = true;
            starts[ip + 1] = true;
        }
        if stops(*instruction) {
            starts[ip + 1] = true;
        }
    }
    for handler in &function.handlers {
        starts[handler.start.index()] = true;
        starts[handler.end.index()] = true;
        for target in entries(handler) {
            starts[target] = true;
        }
    }
    let offsets: Vec<_> = starts[..instructions.len()]
        .iter()
        .enumerate()
        .filter_map(|(ip, &start)| start.then_some(ip))
        .collect();
    let mut at = vec![offsets.len(); instructions.len() + 1];
    for (index, &start) in offsets.iter().enumerate() {
        let end = offsets
            .get(index + 1)
            .copied()
            .unwrap_or(instructions.len());
        at[start..end].fill(index);
    }
    let blocks = offsets
        .iter()
        .enumerate()
        .map(|(index, &start)| {
            let end = offsets
                .get(index + 1)
                .copied()
                .unwrap_or(instructions.len());
            let last = instructions[end - 1];
            let mut successors = Vec::new();
            if !stops(last) && end < instructions.len() {
                successors.push(Edge {
                    block: at[end],
                    taken: false,
                });
            }
            if let Some(target) = target(&last)
                && target.index() < instructions.len()
            {
                successors.push(Edge {
                    block: at[target.index()],
                    taken: true,
                });
            }
            if let I::FinallyReturn { state } = last {
                // One body resumes any saved call with the same continuation slot.
                for (ip, instruction) in instructions.iter().enumerate() {
                    if matches!(instruction, I::FinallyCall { state: saved, .. } if *saved == state)
                        && ip + 1 < instructions.len()
                    {
                        successors.push(Edge {
                            block: at[ip + 1],
                            taken: false,
                        });
                    }
                }
            }
            let handlers = function
                .handlers
                .iter()
                .enumerate()
                .filter_map(|(i, handler)| {
                    (handler.start.index() <= start && start < handler.end.index()).then_some(i)
                })
                .collect();
            Block {
                instructions: start..end,
                successors,
                handlers,
            }
        })
        .collect();
    (blocks, at)
}

fn stops(instruction: I) -> bool {
    matches!(
        instruction,
        I::Jump { .. }
            | I::FinallyCall { .. }
            | I::FinallyReturn { .. }
            | I::Return { .. }
            | I::ReturnChecked { .. }
            | I::ReturnReference { .. }
            | I::ReturnMissing
            | I::ReturnNull
            | I::GeneratorReturn { .. }
            | I::Throw { .. }
            | I::Exit { .. }
    )
}

fn entries(handler: &Handler) -> impl Iterator<Item = usize> {
    handler
        .catch
        .into_iter()
        .chain(handler.finally)
        .map(|target| target.index())
}

fn successor_reads(
    function: &Function,
    block: &Block,
    at: &[usize],
    live: &[Vec<bool>],
) -> Vec<bool> {
    let mut reads = vec![false; function.registers as usize];
    for successor in &block.successors {
        union_reads(&mut reads, &live[successor.block]);
    }
    add_handler_reads(function, block, at, live, &mut reads);
    reads
}

fn add_handler_reads(
    function: &Function,
    block: &Block,
    at: &[usize],
    live: &[Vec<bool>],
    reads: &mut [bool],
) {
    for &index in &block.handlers {
        for ip in entries(&function.handlers[index]) {
            union_reads(reads, &live[at[ip]]);
        }
    }
}

fn union_reads(reads: &mut [bool], other: &[bool]) {
    for (read, &also_read) in reads.iter_mut().zip(other) {
        *read |= also_read;
    }
}

/// Treat every clear and handler cleanup as transparent. This proves that
/// omitting two successive clears cannot expose a stale scalar to a reader.
fn live_at_entries(function: &Function, blocks: &[Block], at: &[usize]) -> Vec<Vec<bool>> {
    let mut live = vec![vec![false; function.registers as usize]; blocks.len()];
    loop {
        let mut changed = false;
        for (index, block) in blocks.iter().enumerate().rev() {
            let mut reads = successor_reads(function, block, at, &live);
            for ip in block.instructions.clone().rev() {
                read_before_write(function, function.instructions[ip], &mut reads);
                // A handler can be reached before an instruction writes its result.
                add_handler_reads(function, block, at, &live, &mut reads);
            }
            if reads != live[index] {
                live[index] = reads;
                changed = true;
            }
        }
        if !changed {
            return live;
        }
    }
}

fn read_before_write(function: &Function, instruction: I, reads: &mut [bool]) {
    // These instructions replace the destination on every normal exit.
    match instruction {
        I::LoadConst { dst, .. }
        | I::LoadInt { dst, .. }
        | I::LoadBool { dst, .. }
        | I::LoadNull { dst }
        | I::Move { dst, .. }
        | I::Unary { dst, .. }
        | I::Cast { dst, .. }
        | I::Binary { dst, .. }
        | I::BinaryInt { dst, .. }
        | I::InstanceOf { dst, .. }
        | I::IssetLocal { dst, .. }
        | I::NewArray { dst, .. }
        | I::FetchThis { dst } => reads[dst.index()] = false,
        _ => {}
    }
    if !operand_regs::visit(
        function,
        instruction,
        |reg| reads[reg.index()] = true,
        |_| {},
    ) {
        reads.fill(true);
    }
}

type Facts = Vec<Option<Vec<Fact>>>;

/// Join all normal predecessors and each protected instruction's possible
/// unwind state. Catch inputs lose the exception register and gain empty slots
/// for the cleanup the VM performs before entering the handler.
fn ownership(
    function: &Function,
    constants: &[Constant],
    types: &[Type],
    blocks: &[Block],
    at: &[usize],
) -> (Facts, Facts) {
    let locals = function.locals.len();
    let mut entry = vec![Fact::Empty; function.registers as usize];
    if function.dynamic_scope {
        entry[..locals].fill(Fact::Unknown);
    } else {
        for (index, parameter) in function.parameters.iter().enumerate() {
            entry[index] = if !parameter.by_reference
                && !parameter.variadic
                && parameter
                    .ty
                    .and_then(|ty| types.get(ty.index()))
                    .is_some_and(|ty| {
                        let scalars = TypeMask::NULL
                            .union(TypeMask::BOOL)
                            .union(TypeMask::INT)
                            .union(TypeMask::FLOAT);
                        ty.classes.is_empty() && ty.mask.0 != 0 && scalars.contains(ty.mask)
                    }) {
                Fact::Scalar
            } else {
                Fact::Unknown
            };
        }
        let first = function.parameters.len();
        entry[first..first + usize::from(function.captures)].fill(Fact::Unknown);
    }
    let mut incoming = vec![None; blocks.len()];
    incoming[0] = Some(entry);
    let mut unwound = vec![None; function.handlers.len()];
    let mut queue = VecDeque::from([0]);
    let mut queued = vec![false; blocks.len()];
    queued[0] = true;
    while let Some(index) = queue.pop_front() {
        queued[index] = false;
        let block = &blocks[index];
        let mut facts = incoming[index].clone().unwrap_or_default();
        let mut on_throw = (!block.handlers.is_empty()).then(|| facts.clone());
        let end_instruction = function.instructions[block.instructions.end - 1];
        let mut iterator_exit = None;
        for ip in block.instructions.clone() {
            let instruction = function.instructions[ip];
            match instruction {
                I::IterNext { value, .. } => {
                    iterator_exit = Some((None, (value, facts[value.index()])));
                }
                I::IterNextKeyed { key, value, .. } => {
                    iterator_exit = Some((
                        Some((key, facts[key.index()])),
                        (value, facts[value.index()]),
                    ));
                }
                _ => {}
            }
            transfer(function, constants, &mut facts, instruction);
            if let Some(on_throw) = &mut on_throw {
                for (old, &new) in on_throw.iter_mut().zip(&facts) {
                    *old = (*old).max(new);
                }
            }
        }
        let mut propagate = |successor: usize, facts: &[Fact]| {
            if join(&mut incoming[successor], facts) && !queued[successor] {
                queued[successor] = true;
                queue.push_back(successor);
            }
        };
        for edge in &block.successors {
            let mut edge_facts;
            let null = match end_instruction {
                I::JumpIfNull { value, .. } if edge.taken => Some(value),
                I::JumpIfNotNull { value, .. } if !edge.taken => Some(value),
                _ => None,
            };
            if let Some(reg) = null
                && reg.index() >= locals
                && facts[reg.index()] < Fact::Unknown
            {
                // A raw value that tested null can hold only null or nothing.
                edge_facts = facts.clone();
                edge_facts[reg.index()] = facts[reg.index()].min(Fact::Scalar);
                propagate(edge.block, &edge_facts);
            } else if let Some((key, value)) = iterator_exit
                && edge.taken
            {
                // An exhausted iterator never writes its key or value.
                edge_facts = facts.clone();
                for (reg, fact) in key.into_iter().chain([value]) {
                    if reg.index() >= locals {
                        edge_facts[reg.index()] = fact;
                    }
                }
                propagate(edge.block, &edge_facts);
            } else {
                propagate(edge.block, &facts);
            }
        }
        if let Some(on_throw) = on_throw {
            for &index in &block.handlers {
                if join(&mut unwound[index], &on_throw) {
                    let handler = &function.handlers[index];
                    let mut received = unwound[index].clone().unwrap_or_default();
                    for reg in &handler.cleanup {
                        received[reg.index()] = Fact::Empty;
                    }
                    received[handler.exception.index()] = Fact::Unknown;
                    for ip in entries(handler) {
                        propagate(at[ip], &received);
                    }
                }
            }
        }
    }
    (incoming, unwound)
}

fn join(slot: &mut Option<Vec<Fact>>, facts: &[Fact]) -> bool {
    let Some(previous) = slot else {
        *slot = Some(facts.to_vec());
        return true;
    };
    let mut changed = false;
    for (old, &new) in previous.iter_mut().zip(facts) {
        if new > *old {
            *old = new;
            changed = true;
        }
    }
    changed
}

fn transfer(function: &Function, constants: &[Constant], facts: &mut [Fact], instruction: I) {
    use Fact::{Empty, NonObject, Scalar, Unknown, Value};
    let get = |reg: Reg| facts[reg.index()];
    let binary = |op, left: Fact, right: Fact| {
        use BinaryOp::*;
        match op {
            Identical | NotIdentical | LogicalXor => Scalar,
            Concat => NonObject,
            Coalesce => Unknown,
            _ if left > NonObject || right > NonObject => Unknown,
            Add | BitwiseAnd | BitwiseOr | BitwiseXor if left > Scalar && right > Scalar => {
                NonObject
            }
            _ => Scalar,
        }
    };
    let result = match instruction {
        I::Clear { reg } | I::UnsetLocal { local: reg } => Some((reg, Empty)),
        I::LoadConst { dst, constant } => Some((
            dst,
            match constants.get(constant.index()) {
                Some(
                    Constant::Null | Constant::Bool(_) | Constant::Int(_) | Constant::Float(_),
                ) => Scalar,
                Some(Constant::String(_) | Constant::Array(_)) => NonObject,
                None => Unknown,
            },
        )),
        I::LoadInt { dst, .. }
        | I::LoadBool { dst, .. }
        | I::LoadNull { dst }
        | I::InstanceOf { dst, .. }
        | I::DynamicInstanceOf { dst, .. }
        | I::IssetLocal { dst, .. }
        | I::IssetPath { dst, .. }
        | I::ArrayKeyExists { dst, .. }
        | I::InLiteralArray { dst, .. }
        | I::BuiltinFrame {
            dst,
            op: BuiltinFrame::ArgumentCount | BuiltinFrame::AssertionsDisabled,
        } => Some((dst, Scalar)),
        I::BuiltinUnary { dst, op, .. } => Some((
            dst,
            match op {
                BuiltinUnary::GetType | BuiltinUnary::GetClass => NonObject,
                BuiltinUnary::TypeCheck(_)
                | BuiltinUnary::IsResource
                | BuiltinUnary::Defined
                | BuiltinUnary::ObjectId
                | BuiltinUnary::StrToLower
                | BuiltinUnary::Strlen => Scalar,
                BuiltinUnary::Count | BuiltinUnary::Sizeof => Scalar,
            },
        )),
        I::NewArray { dst, .. } | I::FetchClassName { dst, .. } => Some((dst, NonObject)),
        I::Move { dst, src } => Some((dst, get(src).clamp(Scalar, Value))),
        I::Assign { local, src } if get(local) != Unknown => Some((local, get(src).max(Scalar))),
        I::Binary { dst, op, lhs, rhs } => Some((dst, binary(op, get(lhs), get(rhs)))),
        I::BinaryInt { dst, op, lhs, .. } => Some((dst, binary(op, get(lhs), Scalar))),
        I::Unary { dst, op, src } if op == UnaryOp::Not || get(src).owns_nothing() => {
            Some((dst, Scalar))
        }
        I::Cast { dst, to, .. } => Some((
            dst,
            match to {
                CastKind::Bool | CastKind::Int | CastKind::Float | CastKind::Void => Scalar,
                CastKind::String | CastKind::Array => NonObject,
                CastKind::Object => Unknown,
            },
        )),
        _ => None,
    };
    let locals = function.locals.len();
    if !operand_regs::visit(
        function,
        instruction,
        |_| {},
        |reg| facts[reg.index()] = Unknown,
    ) {
        facts.fill(Unknown);
    }
    // Both entries move direct arguments out of their caller slots. A throw
    // during binding can still see the earlier owners in the unwind state.
    if let I::CallDirect {
        args, count, dst, ..
    }
    | I::CallNativeDirect {
        args, count, dst, ..
    } = instruction
    {
        for (index, fact) in facts
            .iter_mut()
            .enumerate()
            .skip(args.index())
            .take(count.into())
        {
            if index >= locals && index != dst.index() {
                *fact = Empty;
            }
        }
    }
    if let Some((reg, fact)) = result {
        facts[reg.index()] = fact;
    }
    if function.dynamic_scope {
        facts[..locals].fill(Unknown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phera_bytecode::{ConstId, Target};

    fn function(instructions: Vec<I>, registers: u16) -> Function {
        Function {
            lines: (1..=instructions.len() as u32).collect(),
            instructions: instructions.into(),
            registers,
            ..Function::default()
        }
    }

    #[test]
    fn a_join_requires_both_paths_to_hold_no_owner() {
        let condition = Reg(0);
        let result = Reg(1);
        let mut code = function(
            vec![
                I::JumpIfNot {
                    condition,
                    target: Target(3),
                },
                I::LoadInt {
                    dst: result,
                    value: 1,
                },
                I::Jump { target: Target(4) },
                I::LoadConst {
                    dst: result,
                    constant: ConstId(0),
                },
                I::Clear { reg: result },
                I::ReturnNull,
            ],
            2,
        );
        let mut with_string = code.clone();
        elide(
            &mut with_string,
            &[Constant::String(b"owned".to_vec())],
            &[],
        );
        assert_eq!(with_string.instructions.len(), 6);
        elide(&mut code, &[Constant::Null], &[]);
        assert_eq!(code.instructions.len(), 5);
        assert_eq!(code.instructions[2], I::Jump { target: Target(4) });
        assert_eq!(code.lines, [1, 2, 3, 4, 6]);
    }

    #[test]
    fn a_reader_keeps_the_first_release_that_makes_its_slot_empty() {
        let register = Reg(0);
        let mut code = function(
            vec![
                I::LoadInt {
                    dst: register,
                    value: 1,
                },
                I::Clear { reg: register },
                I::Clear { reg: register },
                I::Return { src: register },
            ],
            1,
        );
        elide(&mut code, &[], &[]);
        assert_eq!(code.instructions.len(), 3);
    }

    #[test]
    fn protected_scalars_can_be_elided_on_both_exits() {
        let scalar = Reg(0);
        let heap = Reg(1);
        let caught = Reg(2);
        let mut code = function(
            vec![
                I::LoadInt {
                    dst: scalar,
                    value: 1,
                },
                I::LoadConst {
                    dst: heap,
                    constant: ConstId(0),
                },
                I::Clear { reg: scalar },
                I::Jump { target: Target(5) },
                I::Clear { reg: scalar },
                I::Clear { reg: heap },
                I::ReturnNull,
            ],
            3,
        );
        code.handlers.push(Handler {
            start: Target(0),
            end: Target(4),
            catch: Some(Target(4)),
            finally: None,
            exception: caught,
            cleanup: vec![scalar, heap],
        });
        elide(&mut code, &[Constant::Array(vec![])], &[]);
        assert_eq!(code.instructions.len(), 5);
        assert_eq!(code.handlers[0].cleanup, [heap]);
        assert_eq!(code.handlers[0].end, Target(3));
        assert_eq!(code.handlers[0].catch, Some(Target(3)));
        assert_eq!(code.instructions[2], I::Jump { target: Target(3) });
        assert_eq!(code.lines, [1, 2, 4, 6, 7]);
    }
    #[test]
    fn a_finally_body_can_change_what_the_saved_continuation_releases() {
        let state = Reg(0);
        let value = Reg(1);
        let mut code = function(
            vec![
                I::LoadInt {
                    dst: value,
                    value: 1,
                },
                I::FinallyCall {
                    state,
                    target: Target(4),
                    reason: phera_bytecode::FinallyEntry::Fallthrough,
                },
                I::Clear { reg: value },
                I::ReturnNull,
                I::LoadConst {
                    dst: value,
                    constant: ConstId(0),
                },
                I::FinallyReturn { state },
            ],
            2,
        );
        elide(&mut code, &[Constant::Array(vec![])], &[]);
        assert_eq!(code.instructions.len(), 6);
        assert_eq!(code.instructions[2], I::Clear { reg: value });
    }
    #[test]
    fn an_exhausted_iterator_leaves_its_value_slot_empty() {
        let value = Reg(0);
        let mut code = function(
            vec![
                I::IterNext {
                    iter: phera_bytecode::IterId(0),
                    value,
                    end: Target(3),
                },
                I::Clear { reg: value },
                I::ReturnNull,
                I::Clear { reg: value },
                I::ReturnNull,
            ],
            1,
        );
        elide(&mut code, &[], &[]);
        assert_eq!(code.instructions.len(), 4);
        assert_eq!(code.instructions[1], I::Clear { reg: value });
    }

    #[test]
    fn testing_a_copied_value_for_null_proves_ownership_on_one_edge() {
        let local = Reg(0);
        let value = Reg(1);
        let mut code = function(
            vec![
                I::Move {
                    dst: value,
                    src: local,
                },
                I::JumpIfNotNull {
                    value,
                    target: Target(4),
                },
                I::Clear { reg: value },
                I::ReturnNull,
                I::Clear { reg: value },
                I::ReturnNull,
            ],
            2,
        );
        code.locals = vec![phera_bytecode::NameId(0)];
        code.dynamic_scope = true;
        elide(&mut code, &[], &[]);
        assert_eq!(code.instructions.len(), 5);
        assert_eq!(code.instructions[3], I::Clear { reg: value });
    }
}
