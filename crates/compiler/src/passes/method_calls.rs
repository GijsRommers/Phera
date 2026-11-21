//! Method calls whose arguments are all locals or literals get one
//! `CallMethod` in front of their `InitMethodCall`, `Send`s and `Call`.
//!
//! ```php
//! $sum += $point->add($i, 2);
//! ```
//!
//! ```text
//! before                              after
//! InitMethodCall $point, add          LoadInt r6, 2
//! SendLocal $i                        CallMethod site#0, r7
//! LoadInt r6, 2                       InitMethodCall $point, add
//! SendValue r6                        SendLocal $i
//! Call r7                             SendValue r6
//!                                     Call r7
//! ```
//!
//! `CallMethod` enters a method written in PHP with the arguments the site
//! names, and the caller goes on after the `Call`. Where it does not (a
//! native method, `__call`, a by-reference parameter, a type the value
//! fails), the loop simply goes on with the next instruction: the call
//! written out the general way, which applies every rule, so there is one
//! definition of the behaviour.
//!
//! PHP finds the method before it evaluates the arguments: an undefined
//! method throws before an argument with a side effect runs. Loading a
//! literal or reading a local has no effect a program can see, so for these
//! calls the literal loads may come before `CallMethod`, and finding the
//! method and reading the arguments may happen together; any argument
//! computed some other way keeps the general sequence alone. The undefined
//! local's warning comes from the general sequence, after the lookup, as in
//! PHP. Because the call stays where it was, a `try` around it covers it
//! either way.

use std::collections::HashSet;

use phera_bytecode::{Function, Instruction, MethodSite, MethodSiteId, Reg, SiteArgument, Target};

/// A call that gets a `CallMethod`: where its instructions are.
struct FoundCall {
    init: usize,
    /// What its `InitMethodCall` and `Call` say.
    receiver: Reg,
    method: phera_bytecode::NameId,
    dst: Reg,
    result: phera_bytecode::CallResult,
    /// The literal loads between `InitMethodCall` and `Call`.
    loads: Vec<usize>,
    /// Each `Send`, with what the site records for it.
    sends: Vec<(usize, SiteArgument)>,
    call: usize,
    /// Literal loads into a register the call already loaded, with the
    /// send that reads them: each gets a register of its own.
    renamed: Vec<(usize, usize)>,
}

/// Give the eligible method calls of one function a `CallMethod`, after
/// its other passes have run.
pub(crate) fn rewrite(function: &mut Function) {
    let mut calls = find_calls(function);
    calls.retain_mut(|call| give_own_registers(function, call));
    if calls.is_empty() {
        return;
    }
    let old = function.instructions.to_vec();
    let old_lines = std::mem::take(&mut function.lines);
    let line = |index: usize| old_lines.get(index).copied().unwrap_or(1);
    let mut instructions = Vec::with_capacity(old.len() + calls.len());
    let mut lines = Vec::with_capacity(old.len() + calls.len());
    // Where each old instruction went, and one past the end.
    let mut moved = vec![0u32; old.len() + 1];
    let mut calls = calls.into_iter().peekable();
    let mut index = 0;
    while index < old.len() {
        let Some(call) = calls.next_if(|call| call.init == index) else {
            moved[index] = instructions.len() as u32;
            instructions.push(old[index]);
            lines.push(line(index));
            index += 1;
            continue;
        };
        // A jump to the call's start lands on its literal loads, which now
        // come first.
        for slot in &mut moved[call.init..=call.call] {
            *slot = instructions.len() as u32;
        }
        for &load in &call.loads {
            moved[load] = instructions.len() as u32;
            instructions.push(old[load]);
            lines.push(line(load));
        }
        let FoundCall {
            receiver,
            method,
            dst,
            result,
            ..
        } = call;
        let site = MethodSiteId(function.method_sites.len() as u32);
        let first_argument = function.site_arguments.len() as u32;
        function
            .site_arguments
            .extend(call.sends.iter().map(|&(_, argument)| argument));
        instructions.push(Instruction::CallMethod { site, dst, result });
        lines.push(line(call.call));
        // The general sequence. A jump to the call keeps landing on its
        // start (set above), not on this `InitMethodCall`.
        instructions.push(old[call.init]);
        lines.push(line(call.init));
        for written_out in call
            .sends
            .iter()
            .map(|&(send, _)| send)
            .chain(std::iter::once(call.call))
        {
            moved[written_out] = instructions.len() as u32;
            instructions.push(old[written_out]);
            lines.push(line(written_out));
        }
        function.method_sites.push(MethodSite {
            receiver,
            method,
            first_argument,
            arguments: call.sends.len() as u16,
            after: Target(instructions.len() as u32),
        });
        index = call.call + 1;
    }
    moved[old.len()] = instructions.len() as u32;
    let renumber = |target: &mut Target| {
        target.0 = moved.get(target.index()).copied().unwrap_or(target.0);
    };
    for instruction in &mut instructions {
        if let Some(target) = crate::passes::optimize::target_mut(instruction) {
            renumber(target);
        }
    }
    for handler in &mut function.handlers {
        renumber(&mut handler.start);
        renumber(&mut handler.end);
        if let Some(catch) = &mut handler.catch {
            renumber(catch);
        }
        if let Some(finally) = &mut handler.finally {
            renumber(finally);
        }
    }
    function.instructions = instructions.into();
    function.lines = lines;
}

/// `CallMethod` reads every argument at the call, so a literal loaded into
/// a register the call loaded before (the optimizer reuses temporaries:
/// `$o->m(1, 2)` may load both into one) moves to a register of its own,
/// with the send that reads it. `false`, having changed nothing for the
/// call, when the function has no register left.
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

/// The call that starts at `init`, if it can have a `CallMethod`.
fn plain_call(
    function: &Function,
    init: usize,
    locals: usize,
    targets: &HashSet<usize>,
) -> Option<FoundCall> {
    let Instruction::InitMethodCall { receiver, method } = function.instructions[init] else {
        return None;
    };
    // The literal registers loaded so far, with where the last load is
    // and whether it moves to a register of its own.
    let mut literals: Vec<(Reg, usize, bool)> = Vec::new();
    let mut loads = Vec::new();
    let mut sends = Vec::new();
    let mut renamed = Vec::new();
    for index in init + 1..function.instructions.len() {
        if targets.contains(&index) {
            return None;
        }
        match function.instructions[index] {
            // A load into the receiver's register keeps the general
            // sequence alone; a second load into one register moves to its
            // own (see `give_own_registers`).
            Instruction::LoadInt { dst, .. }
            | Instruction::LoadConst { dst, .. }
            | Instruction::LoadNull { dst }
            | Instruction::LoadBool { dst, .. }
                if dst.index() >= locals && dst != receiver =>
            {
                let again = literals.iter().any(|(register, ..)| *register == dst);
                literals.push((dst, index, again));
                loads.push(index);
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
            Instruction::Call { dst, result } => {
                return (sends.len() <= usize::from(u16::MAX)).then_some(FoundCall {
                    init,
                    receiver,
                    method,
                    dst,
                    result,
                    loads,
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

#[cfg(test)]
mod tests {
    use phera_bytecode::{Function, Instruction, Target};

    fn function(source: &str, name: &str) -> Function {
        let unit = crate::compile(source, "calls.php").expect("compiles");
        unit.functions
            .into_iter()
            .find(|function| function.name == name)
            .expect("function")
    }

    /// How many method calls of `name` got a `CallMethod`, and how many
    /// kept only the general sequence.
    fn calls(source: &str, name: &str) -> (usize, usize) {
        let function = function(source, name);
        let count = |test: fn(&Instruction) -> bool| {
            function.instructions.iter().filter(|i| test(i)).count()
        };
        let fused = count(|i| matches!(i, Instruction::CallMethod { .. }));
        let started = count(|i| matches!(i, Instruction::InitMethodCall { .. }));
        (fused, started - fused)
    }

    #[test]
    fn locals_and_literals_get_a_call_method() {
        let source = "<?php function f($o, $a) { $o->m(); $o->m($a, 1, 'x', null, true); }";
        assert_eq!(calls(source, "f"), (2, 0));
    }

    #[test]
    fn two_literals_read_at_the_call_need_two_registers() {
        let function = function("<?php function f($o) { return $o->m(1, 2); }", "f");
        for site in &function.method_sites {
            let registers: Vec<_> = function.site_arguments[site.argument_range()]
                .iter()
                .map(|argument| argument.register())
                .collect();
            assert!(registers.first() != registers.get(1), "{registers:?}");
        }
    }

    #[test]
    fn arguments_that_could_be_seen_keep_the_general_sequence_alone() {
        let source = r#"<?php function f($o, $a, $b) {
            $o->m(g());
            $o->m($a + 1);
            $o->m(x: $a);
            $o->m(...$b);
            $o->m($o->n($a));
            try { $o->m($a); } catch (Exception $e) {}
        }"#;
        // The inner `$o->n($a)` and the call inside the `try` get one.
        assert_eq!(calls(source, "f"), (2, 5));
    }

    #[test]
    fn the_general_sequence_follows_and_the_site_returns_after_it() {
        let function = function("<?php function f($o, $a) { return $o->m($a, 7); }", "f");
        let [site] = function.method_sites.as_slice() else {
            panic!("one site");
        };
        let call = function
            .instructions
            .iter()
            .position(|i| matches!(i, Instruction::CallMethod { .. }))
            .expect("CallMethod");
        let code = &function.instructions;
        assert!(matches!(
            code[call - 1],
            Instruction::LoadInt { value: 7, .. }
        ));
        assert!(matches!(code[call + 1], Instruction::InitMethodCall { .. }));
        assert!(matches!(code[call + 2], Instruction::SendLocal { .. }));
        assert!(matches!(code[call + 3], Instruction::SendValue { .. }));
        assert!(matches!(code[call + 4], Instruction::Call { .. }));
        assert_eq!(site.after, Target(call as u32 + 5));
    }

    #[test]
    fn jumps_and_try_ranges_follow_the_moved_instructions() {
        let source = r#"<?php function f($o, $a) {
            for ($i = 0; $i < 3; $i++) { $o->m($a, 1); }
            try { $o->m(thrower()); } catch (Exception $e) { $o->m($a); } finally { $o->m(); }
            return $o->m($a);
        }"#;
        let function = function(source, "f");
        let length = function.instructions.len();
        for instruction in function.instructions.iter() {
            let mut instruction = *instruction;
            if let Some(target) = crate::passes::optimize::target_mut(&mut instruction) {
                assert!(target.index() <= length, "{instruction:?}");
                assert!(
                    !matches!(
                        function.instructions.get(target.index()),
                        Some(Instruction::InitMethodCall { .. })
                    ) || !matches!(
                        function.instructions.get(target.index().wrapping_sub(1)),
                        Some(Instruction::CallMethod { .. })
                    )
                );
            }
        }
        // The innermost handler still covers the call inside the `try`.
        let handler = function.handlers.first().expect("a handler");
        let protected = &function.instructions[handler.start.index()..handler.end.index()];
        assert!(
            protected
                .iter()
                .any(|i| matches!(i, Instruction::InitMethodCall { .. }))
        );
    }
}
