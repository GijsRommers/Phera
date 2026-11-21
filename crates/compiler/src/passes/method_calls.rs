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
    /// Literal loads into a register the call already loaded, with the
    /// send that reads them: each gets a register of its own.
    renamed: Vec<(usize, usize)>,
}

/// Rewrite the eligible method calls of one function, after its other
/// passes have run.
pub(crate) fn rewrite(function: &mut Function) {
    let mut calls = find_calls(function);
    if calls.is_empty() {
        return;
    }
    calls.retain_mut(|call| give_own_registers(function, call));
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

/// `CallMethod` reads every argument at the call, so a literal loaded into
/// a register the call loaded before (the optimizer reuses temporaries:
/// `$o->m(1, 2)` may load both into one) moves to a register of its own,
/// with the send that reads it.
/// `false`, having changed nothing for the call, when the function has no
/// register left.
fn give_own_registers(function: &mut Function, call: &mut FoundCall) -> bool {
    let needed = u16::try_from(call.renamed.len()).ok();
    if needed
        .and_then(|needed| function.registers.checked_add(needed))
        .is_none()
    {
        return false;
    }
    for &(load, send) in &call.renamed {
        let fresh = Reg(function.registers);
        function.registers += 1;
        function.instructions[load] = match function.instructions[load] {
            Instruction::LoadInt { value, .. } => Instruction::LoadInt { dst: fresh, value },
            Instruction::LoadConst { constant, .. } => Instruction::LoadConst {
                dst: fresh,
                constant,
            },
            Instruction::LoadNull { .. } => Instruction::LoadNull { dst: fresh },
            Instruction::LoadBool { value, .. } => Instruction::LoadBool { dst: fresh, value },
            other => other,
        };
        function.instructions[send] = Instruction::SendValue { src: fresh };
        for (index, argument) in &mut call.sends {
            if *index == send {
                *argument = SiteArgument::Value(fresh);
            }
        }
    }
    true
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
    let Instruction::InitMethodCall { receiver, .. } = function.instructions[init] else {
        return None;
    };
    // The literal registers loaded so far, with where the last load is
    // and whether it moves to a register of its own.
    let mut literals: Vec<(Reg, usize, bool)> = Vec::new();
    let mut sends = Vec::new();
    let mut renamed = Vec::new();
    for index in init + 1..function.instructions.len() {
        if targets.contains(&index) {
            return None;
        }
        match function.instructions[index] {
            Instruction::Nop => {}
            // A load into the receiver's register keeps the general
            // sequence; a second load into one register moves to its own
            // (see `give_own_registers`).
            Instruction::LoadInt { dst, .. }
            | Instruction::LoadConst { dst, .. }
            | Instruction::LoadNull { dst }
            | Instruction::LoadBool { dst, .. }
                if dst.index() >= locals && dst != receiver =>
            {
                let again = literals.iter().any(|(register, ..)| *register == dst);
                literals.push((dst, index, again));
            }
            Instruction::SendLocal { local } if local.index() < locals => {
                sends.push((index, SiteArgument::Local(local)));
            }
            Instruction::SendValue { src } => {
                let (_, load, again) = *literals
                    .iter()
                    .rev()
                    .find(|(register, ..)| *register == src)?;
                if again {
                    renamed.push((load, index));
                }
                sends.push((index, SiteArgument::Value(src)));
            }
            Instruction::Call { .. } => {
                return (sends.len() <= usize::from(u16::MAX)).then_some(FoundCall {
                    init,
                    sends,
                    call: index,
                    renamed,
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

#[cfg(test)]
mod tests {
    use phera_bytecode::Instruction;

    /// How many calls of the function named `name` became `CallMethod`, and
    /// how many kept `InitMethodCall` in the code itself (the fallbacks,
    /// after the code, are not counted).
    fn calls(source: &str, name: &str) -> (usize, usize) {
        let unit = crate::compile(source, "calls.php").expect("compiles");
        let function = unit
            .functions
            .iter()
            .find(|function| function.name == name)
            .expect("function");
        let end = function
            .method_sites
            .iter()
            .map(|site| site.fallback.index())
            .min()
            .unwrap_or(function.instructions.len());
        let code = &function.instructions[..end];
        let count = |test: fn(&Instruction) -> bool| code.iter().filter(|i| test(i)).count();
        (
            count(|i| matches!(i, Instruction::CallMethod { .. })),
            count(|i| matches!(i, Instruction::InitMethodCall { .. })),
        )
    }

    #[test]
    fn locals_and_literals_make_one_instruction() {
        let source = "<?php function f($o, $a) { $o->m(); $o->m($a, 1, 'x', null, true); }";
        assert_eq!(calls(source, "f"), (2, 0));
    }

    #[test]
    fn two_literals_read_at_the_call_need_two_registers() {
        let unit = crate::compile("<?php function f($o) { return $o->m(1, 2); }", "f.php")
            .expect("compiles");
        let function = unit.functions.iter().find(|f| f.name == "f").expect("f");
        for site in &function.method_sites {
            let registers: Vec<_> = function.site_arguments[site.argument_range()]
                .iter()
                .map(|argument| argument.register())
                .collect();
            assert!(registers.first() != registers.get(1), "{registers:?}");
        }
    }

    #[test]
    fn arguments_that_could_be_seen_keep_the_general_sequence() {
        let source = r#"<?php function f($o, $a, $b) {
            $o->m(g());
            $o->m($a + 1);
            $o->m(x: $a);
            $o->m(...$b);
            $o->m($o->n($a));
            try { $o->m($a); } catch (Exception $e) {}
        }"#;
        // Only the inner `$o->n($a)` of the fifth call is one instruction.
        assert_eq!(calls(source, "f"), (1, 6));
    }

    #[test]
    fn a_fallback_jumps_back_after_its_call() {
        let unit = crate::compile("<?php function f($o, $a) { return $o->m($a); }", "f.php")
            .expect("compiles");
        let function = unit.functions.iter().find(|f| f.name == "f").expect("f");
        let [site] = function.method_sites.as_slice() else {
            panic!("one site");
        };
        let call = function
            .instructions
            .iter()
            .position(|i| matches!(i, Instruction::CallMethod { .. }))
            .expect("CallMethod");
        let fallback = &function.instructions[site.fallback.index()..];
        assert!(matches!(fallback[0], Instruction::InitMethodCall { .. }));
        assert!(matches!(fallback[1], Instruction::SendLocal { .. }));
        assert!(matches!(fallback[2], Instruction::Call { .. }));
        assert_eq!(
            fallback[3],
            Instruction::Jump {
                target: phera_bytecode::Target(call as u32 + 1)
            }
        );
    }
}
