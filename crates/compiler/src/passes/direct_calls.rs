//! Calls to a function this file declares at its top level become one
//! `CallDirect` instead of `InitCall`, one `Send` per argument and `Call`.
//!
//! ```php
//! function add($a, $b) { return $a + $b; }
//! echo add($x, 2);
//! ```
//!
//! `add` is declared before the file's first line runs and can never be
//! declared again, so the call needs no lookup and the "undefined function"
//! check that `InitCall` makes before the arguments is never needed. Each
//! argument is put where `Send` would have read it, into one register of a
//! small argument area after the function's own registers:
//!
//! ```text
//! before                              after
//! InitCall add                        Nop
//! Move r5, $x ; SendValue r5          Move r5, $x ; Move r9, r5
//! LoadInt r6, 2 ; SendValue r6        LoadInt r6, 2 ; Move r10, r6
//! Call r7                             CallDirect add, args r9, count 2, r7
//! ```
//!
//! A call written inside another call's arguments uses the next area, so
//! `add(1, add(2, 3))` keeps the outer call's first argument while the inner
//! call runs. Calls with named or spread arguments, more arguments than
//! parameters, a variadic or by-reference parameter keep the general
//! sequence.

use std::collections::HashMap;

use phera_bytecode::{FetchMode, Function, FunctionId, Instruction, Reg, Symbol};

/// What the pass needs to know about a function the file declares at its
/// top level.
#[derive(Clone, Copy)]
pub(crate) struct DirectCallee {
    pub(crate) function: FunctionId,
    pub(crate) parameters: usize,
    /// A variadic or by-reference parameter: arguments need the general path.
    pub(crate) needs_general_call: bool,
}

/// The top-level functions of a file, by lowercased name.
pub(crate) type DirectCallees = HashMap<String, DirectCallee>;

/// A call whose arguments are being scanned.
struct OpenCall {
    /// `Some` for a call to a top-level function of this file.
    callee: Option<DirectCallee>,
    init: usize,
    /// How many enclosing calls to top-level functions are open.
    level: usize,
    sends: Vec<usize>,
    /// A named or spread argument was seen.
    general: bool,
}

/// A call that will become `CallDirect`.
struct DirectCall {
    callee: DirectCallee,
    init: usize,
    level: usize,
    sends: Vec<usize>,
    call: usize,
}

/// Rewrite the eligible calls of one function. Returns where the argument
/// areas start, or `None` when nothing was rewritten.
pub(crate) fn rewrite(
    function: &mut Function,
    symbols: &[Symbol],
    callees: &DirectCallees,
) -> Option<usize> {
    let calls = find_direct_calls(function, symbols, callees);
    if calls.is_empty() {
        return None;
    }
    let first = function.registers as usize;
    let mut areas = Vec::new();
    for call in &calls {
        if areas.len() <= call.level {
            areas.resize(call.level + 1, 0);
        }
        areas[call.level] = areas[call.level].max(call.sends.len());
    }
    let starts: Vec<usize> = areas
        .iter()
        .scan(first, |next, size| {
            let start = *next;
            *next += size;
            Some(start)
        })
        .collect();
    let end = first + areas.iter().sum::<usize>();
    let registers = u16::try_from(end).ok()?;
    function.registers = registers;
    for call in calls {
        let area = starts[call.level];
        rewrite_call(function, &call, area);
    }
    for handler in &mut function.handlers {
        handler
            .cleanup
            .extend((first..end).map(|index| Reg(index as u16)));
    }
    Some(first)
}

/// Where an argument was computed into a temporary just before being copied
/// into the argument area, compute it into the area instead:
/// `BinaryInt r5 = $n - 1; Move r9, r5` becomes `BinaryInt r9 = $n - 1`.
/// Returns whether anything changed, leaving `Nop`s behind.
pub(crate) fn write_arguments_in_place(function: &mut Function, area: usize) -> bool {
    let mut changed = false;
    let targets: std::collections::HashSet<_> = function
        .instructions
        .iter()
        .filter_map(crate::passes::optimize::target)
        .map(|target| target.index())
        .collect();
    for index in 1..function.instructions.len() {
        let Instruction::Move { dst, src } = function.instructions[index] else {
            continue;
        };
        if dst.index() < area
            || src.index() < function.locals.len()
            || src.index() >= area
            || targets.contains(&index)
            || !crate::passes::integer_operands::dead_after(function, index, src)
        {
            continue;
        }
        if let Some(producer) = retarget(function.instructions[index - 1], src, dst) {
            function.instructions[index - 1] = producer;
            function.instructions[index] = Instruction::Nop;
            changed = true;
        }
    }
    changed
}

/// The same instruction writing `to` instead of `from`, for instructions
/// whose only effect on registers is writing their destination.
fn retarget(instruction: Instruction, from: Reg, to: Reg) -> Option<Instruction> {
    use Instruction as I;
    let retargeted = match instruction {
        I::LoadInt { dst, value } if dst == from => I::LoadInt { dst: to, value },
        I::LoadConst { dst, constant } if dst == from => I::LoadConst { dst: to, constant },
        I::LoadNull { dst } if dst == from => I::LoadNull { dst: to },
        I::LoadBool { dst, value } if dst == from => I::LoadBool { dst: to, value },
        I::Move { dst, src } if dst == from => I::Move { dst: to, src },
        I::Binary { op, dst, lhs, rhs } if dst == from => I::Binary {
            op,
            dst: to,
            lhs,
            rhs,
        },
        I::BinaryInt { op, dst, lhs, rhs } if dst == from => I::BinaryInt {
            op,
            dst: to,
            lhs,
            rhs,
        },
        I::Call { dst, result } if dst == from => I::Call { dst: to, result },
        I::CallDirect {
            function,
            args,
            count,
            dst,
            result,
        } if dst == from => I::CallDirect {
            function,
            args,
            count,
            dst: to,
            result,
        },
        _ => return None,
    };
    Some(retargeted)
}

/// Scan the function once, matching every call start with its `Send`s and
/// its `Call`.
fn find_direct_calls(
    function: &Function,
    symbols: &[Symbol],
    callees: &DirectCallees,
) -> Vec<DirectCall> {
    let mut open: Vec<OpenCall> = Vec::new();
    let mut found = Vec::new();
    for (index, instruction) in function.instructions.iter().enumerate() {
        match instruction {
            Instruction::InitCall { function } => {
                let callee = symbols
                    .get(function.index())
                    .and_then(|symbol| callees.get(&symbol.name))
                    .copied();
                open.push(open_call(&open, callee, index));
            }
            Instruction::InitDynamicCall { .. }
            | Instruction::InitMethodCall { .. }
            | Instruction::InitStaticCall { .. }
            | Instruction::DynamicMethodCall { .. }
            | Instruction::StaticCall { .. }
            | Instruction::New { .. }
            | Instruction::NewDynamic { .. } => open.push(open_call(&open, None, index)),
            Instruction::SendValue { .. }
            | Instruction::SendLocal { .. }
            | Instruction::SendPath { .. } => {
                if let Some(call) = open.last_mut() {
                    call.sends.push(index);
                }
            }
            Instruction::SendSpread { .. } | Instruction::NameArgument { .. } => {
                if let Some(call) = open.last_mut() {
                    call.general = true;
                }
            }
            Instruction::Call { .. } => {
                if let Some(call) = open.pop().and_then(|call| direct_call(call, index)) {
                    found.push(call);
                }
            }
            Instruction::MakeCallable { .. } => {
                open.pop();
            }
            _ => {}
        }
    }
    found
}

/// Start tracking a call; its argument area is one deeper than the
/// enclosing direct call's.
fn open_call(open: &[OpenCall], callee: Option<DirectCallee>, init: usize) -> OpenCall {
    OpenCall {
        callee,
        init,
        level: open.iter().filter(|call| call.callee.is_some()).count(),
        sends: Vec::new(),
        general: false,
    }
}

/// The finished call, if it can be a `CallDirect`.
fn direct_call(call: OpenCall, index: usize) -> Option<DirectCall> {
    let callee = call.callee?;
    let fits = !call.general && !callee.needs_general_call && call.sends.len() <= callee.parameters;
    fits.then_some(DirectCall {
        callee,
        init: call.init,
        level: call.level,
        sends: call.sends,
        call: index,
    })
}

/// Replace one call's instructions: the start disappears, each `Send`
/// copies its argument into the area, `Call` becomes `CallDirect`.
fn rewrite_call(function: &mut Function, call: &DirectCall, area: usize) {
    function.instructions[call.init] = Instruction::Nop;
    for (position, &send) in call.sends.iter().enumerate() {
        let dst = Reg((area + position) as u16);
        function.instructions[send] = match function.instructions[send] {
            Instruction::SendValue { src } => Instruction::Move { dst, src },
            Instruction::SendLocal { local } => Instruction::Move { dst, src: local },
            Instruction::SendPath { path } => Instruction::Fetch {
                dst,
                path,
                mode: FetchMode::Read,
            },
            other => other,
        };
    }
    if let Instruction::Call { dst, result } = function.instructions[call.call] {
        function.instructions[call.call] = Instruction::CallDirect {
            function: call.callee.function,
            args: Reg(area as u16),
            count: call.sends.len() as u16,
            dst,
            result,
        };
    }
}
