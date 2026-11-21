//! Method calls whose arguments are all locals or literals become one
//! `CallMethod` instead of `InitMethodCall`, one `Send` per argument and
//! `Call`.
//!
//! ```php
//! $sum += $point->add($i, 2);
//! ```
//!
//! ```text
//! before                              after
//! InitMethodCall $point, add          Nop
//! SendLocal $i                        Nop
//! LoadInt r6, 2                       LoadInt r6, 2
//! SendValue r6                        Nop
//! Call r7                             CallMethod site#0, r7
//!                                     ...
//!                                     site#0's fallback, after the code:
//!                                     InitMethodCall $point, add
//!                                     SendLocal $i
//!                                     SendValue r6
//!                                     Call r7
//!                                     Jump back after CallMethod
//! ```
//!
//! PHP finds the method before it evaluates the arguments: an undefined
//! method throws before an argument with a side effect runs. Reading a
//! local or a literal has no effect PHP code can see before the call, so
//! for these calls finding the method and reading the arguments may happen
//! together; any argument computed some other way keeps the general
//! sequence. Where `CallMethod` declines (a magic `__call`, a by-reference
//! parameter, a type test the value fails), it jumps to the call written
//! out the general way after the function's code, and the VM runs that
//! with every rule; the local's warning for being undefined, for instance,
//! comes after the lookup there, as in PHP.
//!
//! Calls inside a `try` keep the general sequence: the fallback lies
//! outside the protected range, where the `catch` would not see an
//! exception the call throws.

use std::collections::HashSet;

use phera_bytecode::{Function, Instruction, MethodSite, MethodSiteId, Reg, SiteArgument, Target};

/// A call that becomes `CallMethod`: where its instructions are.
struct FoundCall {
    init: usize,
    /// Each `Send`, with what the site records for it.
    sends: Vec<(usize, SiteArgument)>,
    call: usize,
}

/// Rewrite the eligible method calls of one function, after its other
/// passes have run.
pub(crate) fn rewrite(function: &mut Function) {
    let calls = find_calls(function);
    if calls.is_empty() {
        return;
    }
    // The general sequences, kept for the fallbacks before they are
    // replaced.
    let originals: Vec<Vec<(Instruction, u32)>> = calls
        .iter()
        .map(|call| {
            std::iter::once(call.init)
                .chain(call.sends.iter().map(|&(send, _)| send))
                .chain(std::iter::once(call.call))
                .map(|index| (function.instructions[index], line(function, index)))
                .collect()
        })
        .collect();
    for call in &calls {
        let site = MethodSiteId(function.method_sites.len() as u32);
        let Instruction::InitMethodCall { receiver, method } = function.instructions[call.init]
        else {
            continue;
        };
        let first_argument = function.site_arguments.len() as u32;
        for &(send, argument) in &call.sends {
            function.site_arguments.push(argument);
            function.instructions[send] = Instruction::Nop;
        }
        function.method_sites.push(MethodSite {
            receiver,
            method,
            first_argument,
            arguments: call.sends.len() as u16,
            // Set below, once the code has its final positions.
            fallback: Target(0),
        });
        function.instructions[call.init] = Instruction::Nop;
        if let Instruction::Call { dst, result } = function.instructions[call.call] {
            function.instructions[call.call] = Instruction::CallMethod { site, dst, result };
        }
    }
    if function.handlers.is_empty() {
        crate::passes::optimize::compact_nops(function);
    }
    append_fallbacks(function, &originals);
}

/// After the code: each site's general sequence and a jump back after its
/// `CallMethod`.
fn append_fallbacks(function: &mut Function, originals: &[Vec<(Instruction, u32)>]) {
    let positions: Vec<usize> = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            matches!(instruction, Instruction::CallMethod { .. }).then_some(index)
        })
        .collect();
    let mut instructions = function.instructions.to_vec();
    for (position, original) in positions.iter().zip(originals) {
        let Instruction::CallMethod { site, .. } = instructions[*position] else {
            continue;
        };
        function.method_sites[site.index()].fallback = Target(instructions.len() as u32);
        let call_line = original.last().map_or(1, |(_, line)| *line);
        for &(instruction, line) in original {
            instructions.push(instruction);
            function.lines.push(line);
        }
        instructions.push(Instruction::Jump {
            target: Target(*position as u32 + 1),
        });
        function.lines.push(call_line);
    }
    function.instructions = instructions.into();
}

fn line(function: &Function, index: usize) -> u32 {
    function.lines.get(index).copied().unwrap_or(1)
}

/// The method calls whose instructions between `InitMethodCall` and `Call`
/// are only sends of locals, literal loads and the sends of those literals.
fn find_calls(function: &Function) -> Vec<FoundCall> {
    let targets: HashSet<usize> = function
        .instructions
        .iter()
        .filter_map(crate::passes::optimize::target)
        .map(|target| target.index())
        .collect();
    let locals = function.locals.len();
    let mut found = Vec::new();
    let mut index = 0;
    while index < function.instructions.len() {
        if let Instruction::InitMethodCall { .. } = function.instructions[index]
            && let Some(call) = plain_call(function, index, locals, &targets)
        {
            index = call.call + 1;
            found.push(call);
            continue;
        }
        index += 1;
    }
    found
}

/// The call that starts at `init`, if it can be one `CallMethod`.
fn plain_call(
    function: &Function,
    init: usize,
    locals: usize,
    targets: &HashSet<usize>,
) -> Option<FoundCall> {
    if protected(function, init) {
        return None;
    }
    let mut literals: Vec<Reg> = Vec::new();
    let mut sends = Vec::new();
    for index in init + 1..function.instructions.len() {
        if targets.contains(&index) {
            return None;
        }
        match function.instructions[index] {
            Instruction::Nop => {}
            Instruction::LoadInt { dst, .. }
            | Instruction::LoadConst { dst, .. }
            | Instruction::LoadNull { dst }
            | Instruction::LoadBool { dst, .. }
                if dst.index() >= locals =>
            {
                literals.push(dst);
            }
            Instruction::SendLocal { local } if local.index() < locals => {
                sends.push((index, SiteArgument::Local(local)));
            }
            Instruction::SendValue { src } if literals.contains(&src) => {
                sends.push((index, SiteArgument::Value(src)));
            }
            Instruction::Call { .. } => {
                return (sends.len() <= usize::from(u16::MAX)).then_some(FoundCall {
                    init,
                    sends,
                    call: index,
                });
            }
            _ => return None,
        }
    }
    None
}

/// Whether a `try` block covers the instruction.
fn protected(function: &Function, index: usize) -> bool {
    function
        .handlers
        .iter()
        .any(|handler| handler.start.index() <= index && index < handler.end.index())
}
