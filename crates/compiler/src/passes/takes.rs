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
//! The general sequence of a method site is left alone: the site reads its
//! literal temporaries itself, before that sequence runs.

use std::collections::HashSet;

use phera_bytecode::{Function, Instruction, Reg, Target};

/// Rewrite the takes of one function, after its other passes (and the
/// method-call pass) have run.
pub(crate) fn rewrite(function: &mut Function) {
    let targets: HashSet<usize> = function
        .instructions
        .iter()
        .filter_map(crate::passes::optimize::target)
        .map(|target| target.index())
        .collect();
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
    if changed {
        remove_nops(function);
    }
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
