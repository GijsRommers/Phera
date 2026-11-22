//! A temporary read once and then cleared is taken by its reader instead.
//!
//! ```text
//! before                          after
//! Assign $total, r7               AssignTake $total, r7
//! Clear r7
//! SendValue r9                    SendTake r9
//! Call r10                        Call r10
//! Clear r9
//! ```
//!
//! The compiler clears a temporary right after its last use, so a read
//! followed by a `Clear` of the same register, with nothing between them
//! that reads or writes it and no jump into the stretch, is that last use.
//! The taking form leaves the temporary empty, as the `Clear` did, so the
//! value is moved instead of copied and released, and the `Clear` goes.
//!
//! A taken temporary that a `LoadConst` filled just before, with nothing
//! between that reads it, is read from the literal in place:
//!
//! ```text
//! LoadConst r7, c3                AssignConst $name, c3
//! AssignTake $name, r7
//! LoadConst r9, c4                SendConst c4
//! SendTake r9
//! ```
//!
//! The same holds for a plain `Assign` or `SendValue` of the literal, whose
//! `Clear` the optimizer drops because releasing a literal frees nothing,
//! when the temporary is provably not read again. Not loading it leaves
//! the temporary with what it held before: by the same rule, a value whose
//! release frees nothing, or nothing at all.
//!
//! The general sequence of a method site is left alone: the site reads its
//! literal temporaries itself, before that sequence runs.

use std::collections::HashSet;

use phera_bytecode::{ConstId, Function, Instruction, Reg, Target};

use crate::passes::integer_operands::dead_after;

/// Rewrite the takes of one function, after its other passes (and the
/// method-call pass) have run.
pub(crate) fn rewrite(function: &mut Function) {
    let targets = entries(function);
    let locals = function.locals.len();
    let sites = site_sequences(function);
    let mut changed = false;
    for index in 0..function.instructions.len() {
        if sites.contains(&index) {
            continue;
        }
        let (taking, temporary) = match function.instructions[index] {
            Instruction::Assign { local, src } if src.index() >= locals => {
                (Instruction::AssignTake { local, src }, src)
            }
            Instruction::SendValue { src } if src.index() >= locals => {
                (Instruction::SendTake { src }, src)
            }
            _ => continue,
        };
        if let Some(clear) = clear_after(function, index, temporary, &targets) {
            function.instructions[index] = taking;
            function.instructions[clear] = Instruction::Nop;
            changed = true;
        }
    }
    for index in 0..function.instructions.len() {
        if sites.contains(&index) {
            continue;
        }
        let (local, temporary) = match function.instructions[index] {
            Instruction::AssignTake { local, src } => (Some(local), src),
            Instruction::SendTake { src } => (None, src),
            // A literal's `Clear` frees nothing, so the optimizer drops it:
            // the temporary must be proved unread instead.
            Instruction::Assign { local, src }
                if src.index() >= locals && dead_after(function, index, src) =>
            {
                (Some(local), src)
            }
            Instruction::SendValue { src }
                if src.index() >= locals && dead_after(function, index, src) =>
            {
                (None, src)
            }
            _ => continue,
        };
        let Some((load, constant)) = load_before(function, index, temporary, &targets) else {
            continue;
        };
        function.instructions[index] = match local {
            Some(local) => Instruction::AssignConst { local, constant },
            None => Instruction::SendConst { constant },
        };
        function.instructions[load] = Instruction::Nop;
        changed = true;
    }
    if changed {
        remove_nops(function);
    }
    mark_taken_receivers(function);
}

/// A site whose receiver is a temporary cleared right after its `Call`
/// takes the receiver (see [`phera_bytecode::MethodSite`]): the `Clear`
/// moves before the `Call`, where the prepared call holds `$this` already,
/// so a quick entry that moves the receiver goes on at `after` as before.
/// Nothing may jump to the `Clear`, and no `try` range may start or end
/// between the two.
fn mark_taken_receivers(function: &mut Function) {
    let locals = function.locals.len();
    let targets = entries(function);
    for index in 0..function.method_sites.len() {
        let site = function.method_sites[index];
        let clear = site.after.index();
        let Some(call) = clear.checked_sub(1) else {
            continue;
        };
        let bounds = function.handlers.iter().any(|handler| {
            [handler.start, handler.end]
                .iter()
                .any(|edge| edge.index() == clear)
        });
        let takes = site.receiver.index() >= locals
            && clear < function.lines.len()
            && !targets.contains(&clear)
            && !bounds
            && matches!(
                function.instructions.get(call),
                Some(Instruction::Call { .. })
            )
            && matches!(
                function.instructions.get(clear),
                Some(Instruction::Clear { reg }) if *reg == site.receiver
            );
        if takes {
            function.instructions.swap(call, clear);
            function.lines.swap(call, clear);
            function.method_sites[index].after = Target(site.after.0 + 1);
            function.method_sites[index].receiver_taken = true;
        }
    }
}

/// Every position a jump, a `catch` or a `finally` can enter.
fn entries(function: &Function) -> HashSet<usize> {
    let jumps = function
        .instructions
        .iter()
        .filter_map(crate::passes::optimize::target);
    let handlers = function
        .handlers
        .iter()
        .flat_map(|handler| [handler.catch, handler.finally])
        .flatten();
    jumps.chain(handlers).map(|target| target.index()).collect()
}

/// The `LoadConst` that filled `register` before its reader at `index`,
/// and its constant, if only other literal loads, sends and `Clear`s come
/// between and no jump lands there or on the reader.
fn load_before(
    function: &Function,
    index: usize,
    register: Reg,
    targets: &HashSet<usize>,
) -> Option<(usize, ConstId)> {
    use Instruction as I;
    for previous in (0..index).rev() {
        if targets.contains(&(previous + 1)) {
            return None;
        }
        match function.instructions[previous] {
            I::LoadConst { dst, constant } if dst == register => return Some((previous, constant)),
            I::Nop | I::SendConst { .. } => {}
            I::Clear { reg: other }
            | I::SendValue { src: other }
            | I::SendTake { src: other }
            | I::SendLocal { local: other }
            | I::LoadInt { dst: other, .. }
            | I::LoadConst { dst: other, .. }
            | I::LoadNull { dst: other }
            | I::LoadBool { dst: other, .. }
                if other != register => {}
            _ => return None,
        }
    }
    None
}

/// Where `register` is cleared after `index`, if nothing between reads or
/// writes it and no jump lands between.
fn clear_after(
    function: &Function,
    index: usize,
    register: Reg,
    targets: &HashSet<usize>,
) -> Option<usize> {
    for next in index + 1..function.instructions.len() {
        if targets.contains(&next) {
            return None;
        }
        match function.instructions[next] {
            Instruction::Clear { reg } if reg == register => return Some(next),
            instruction if untouched(instruction, register) => {}
            _ => return None,
        }
    }
    None
}

/// Whether `instruction` certainly neither reads nor writes `register`.
/// Only the instructions that come between a send and its `Clear` are
/// known here; any other answers no.
fn untouched(instruction: Instruction, register: Reg) -> bool {
    use Instruction as I;
    match instruction {
        I::Nop => true,
        I::Clear { reg } => reg != register,
        I::SendValue { src } | I::SendTake { src } => src != register,
        I::SendConst { .. } => true,
        I::SendLocal { local } => local != register,
        I::LoadInt { dst, .. }
        | I::LoadConst { dst, .. }
        | I::LoadNull { dst }
        | I::LoadBool { dst, .. } => dst != register,
        I::Call { dst, .. } | I::CallMethod { dst, .. } => dst != register,
        _ => false,
    }
}

/// The instructions of every method site's general sequence (after its
/// `CallMethod`, up to its `after`).
fn site_sequences(function: &Function) -> HashSet<usize> {
    let mut inside = HashSet::new();
    for (index, instruction) in function.instructions.iter().enumerate() {
        if let Instruction::CallMethod { site, .. } = instruction
            && let Some(site) = function.method_sites.get(site.index())
        {
            inside.extend(index + 1..site.after.index());
        }
    }
    inside
}

/// Removes the `Nop`s, renumbering the jump targets, the `try` ranges, the
/// `finally` entries and the method sites' `after`.
fn remove_nops(function: &mut Function) {
    let mut moved = Vec::with_capacity(function.instructions.len() + 1);
    let mut kept = 0u32;
    for instruction in function.instructions.iter() {
        moved.push(kept);
        if !matches!(instruction, Instruction::Nop) {
            kept += 1;
        }
    }
    moved.push(kept);
    let renumber = |target: &mut Target| {
        target.0 = moved.get(target.index()).copied().unwrap_or(target.0);
    };
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
    for site in &mut function.method_sites {
        renumber(&mut site.after);
    }
    // `compact_nops` renumbers the instructions' own targets and the lines.
    crate::passes::optimize::compact_nops(function);
}
