//! Read a local as a path key where it was copied only to be read once.
//!
//! ```php
//! $row[$key]        // Move t, $key; Fetch [Index(t)]      -> Fetch [Index($key)]
//! $a[$i][$j] = 1;   // Move t1, $i; Move t2, $j; AssignPath -> AssignPath [Index($i), Index($j)]
//! ```
//!
//! A path key is a register like any operand, but the compiler copies a local
//! into a temporary first, so that a later operand of the same expression
//! (`$a[$i][$i++]`) cannot change an earlier one. When nothing between the
//! copy and the path instruction can write the local, the path reads the
//! local directly and the copy (and the `Clear` that empties its temporary)
//! disappears.
//!
//! A local is skipped when it may hold a PHP reference (bound by `&`, `global`,
//! `static`, a by-reference parameter or capture, `foreach ... as &`, or passed
//! where a call may take it by reference): the scalar loop falls back to the
//! general path on a reference slot, and reading the local in place would
//! leave the loop on every execution. Functions with dynamic scope are skipped
//! entirely, because `$$name` can bind any of their registers to a reference.
//!
//! The rewrite keeps every path to the consuming instruction passing through
//! the removed copy: the temporary has that one copy as its only assignment,
//! no jump or branch target lies between them, and the consumer is not an
//! exception-handler entry. Removed instructions become `Nop` and are
//! compacted away with their targets remapped.

use std::collections::{HashMap, HashSet};

use phera_bytecode::{Function, Instruction, PathBase, PathId, Reg, Step};

use crate::passes::operand_regs::{operand_regs, path_operand, runs_php_code};
use crate::passes::optimize::{target, target_mut};

/// One removable copy: `Move temp, local` at `producer`, read by the path
/// step of instruction `consumer`.
struct Candidate {
    consumer: usize,
    path: PathId,
    step: usize,
    temp: Reg,
    local: Reg,
    producer: usize,
}

/// Rewrite path keys that are plain copies of locals. Runs after `optimize`,
/// on bytecode with valid register and target indices.
pub(crate) fn optimize(function: &mut Function) {
    if function.instructions.is_empty() {
        return;
    }
    let exposed = reference_locals(function);
    let locals = function.locals.len();
    if exposed.len() == locals {
        return;
    }
    let boundaries: HashSet<usize> = function
        .instructions
        .iter()
        .filter_map(target)
        .map(|target| target.index())
        .collect();
    let handler_entries: HashSet<usize> = function
        .handlers
        .iter()
        .filter_map(|handler| handler.catch.or(handler.finally))
        .map(|target| target.index())
        .collect();
    // Every assignment per register, split from clears: a `Clear` empties a
    // temporary but does not assign it.
    let mut assigns: HashMap<Reg, Vec<usize>> = HashMap::new();
    let mut clears: HashSet<usize> = HashSet::new();
    for (index, instruction) in function.instructions.iter().enumerate() {
        if matches!(instruction, Instruction::Clear { .. }) {
            clears.insert(index);
            continue;
        }
        for register in operand_regs(function, *instruction).writes {
            assigns.entry(register).or_default().push(index);
        }
    }
    let mut candidates: Vec<Candidate> = Vec::new();
    for (index, instruction) in function.instructions.iter().enumerate() {
        let Some(path_id) = path_operand(*instruction) else {
            continue;
        };
        let Some(path) = function.paths.get(path_id.index()) else {
            continue;
        };
        for (position, step) in path.steps.iter().enumerate() {
            let temp = match step {
                Step::Index(register) | Step::DynamicProperty(register) => *register,
                Step::Property(_) | Step::Append => continue,
            };
            if temp.index() < locals {
                continue;
            }
            if let Some(candidate) = candidate(
                function,
                &exposed,
                &boundaries,
                &handler_entries,
                &assigns,
                &clears,
                index,
                path_id,
                position,
                temp,
            ) {
                candidates.push(candidate);
            }
        }
    }
    // Group by temporary: every reader of a temporary must be a candidate
    // consumer, and candidates sharing one path step must name the same
    // local, or that temporary's copies all stay.
    let mut by_temp: HashMap<Reg, Vec<Candidate>> = HashMap::new();
    for candidate in candidates {
        by_temp.entry(candidate.temp).or_default().push(candidate);
    }
    for (temp, group) in by_temp {
        let readers: Vec<usize> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, instruction)| operand_regs(function, **instruction).reads(temp))
            .map(|(index, _)| index)
            .collect();
        let consumers: HashSet<usize> = group.iter().map(|candidate| candidate.consumer).collect();
        // One path step can only name one register: candidates that share a
        // step must agree on the local.
        let mut steps: HashMap<(PathId, usize), Reg> = HashMap::new();
        let same_local = group.iter().all(|candidate| {
            steps
                .insert((candidate.path, candidate.step), candidate.local)
                .is_none_or(|previous| previous == candidate.local)
        });
        if !readers.iter().all(|index| consumers.contains(index)) || !same_local {
            continue;
        }
        for candidate in &group {
            if let Some(path) = function.paths.get_mut(candidate.path.index()) {
                path.steps[candidate.step] = match path.steps[candidate.step] {
                    Step::Index(_) => Step::Index(candidate.local),
                    Step::DynamicProperty(_) => Step::DynamicProperty(candidate.local),
                    step => step,
                };
            }
            // `FetchIndex` also names the key outside the path table.
            if let Instruction::FetchIndex { index, .. } =
                &mut function.instructions[candidate.consumer]
                && *index == temp
            {
                *index = candidate.local;
            }
        }
        // Only the copies go: each candidate's `Move` and every `Clear` of
        // the temporary. Another instruction can also write the temporary
        // (a path base scratch, a call result); it keeps running, it merely
        // writes a register that nobody reads anymore.
        for candidate in &group {
            function.instructions[candidate.producer] = Instruction::Nop;
        }
        for index in &clears {
            if matches!(function.instructions[*index], Instruction::Clear { reg } if reg == temp) {
                function.instructions[*index] = Instruction::Nop;
            }
        }
    }
    compact(function);
}

/// Whether the copy that last assigned `temp` before `consumer` can be
/// dropped for the path step at `consumer`. Every path to `consumer`
/// passes through that copy: no jump or branch target lies between them,
/// so control falls straight through the window.
#[allow(clippy::too_many_arguments)]
fn candidate(
    function: &Function,
    exposed: &HashSet<Reg>,
    boundaries: &HashSet<usize>,
    handler_entries: &HashSet<usize>,
    assigns: &HashMap<Reg, Vec<usize>>,
    clears: &HashSet<usize>,
    consumer: usize,
    path: PathId,
    step: usize,
    temp: Reg,
) -> Option<Candidate> {
    let locals = function.locals.len();
    let producer = assigns
        .get(&temp)?
        .iter()
        .copied()
        .rev()
        .find(|index| *index < consumer)?;
    // No clear of the temporary between the copy and the consumer either.
    if clears
        .iter()
        .any(|index| *index > producer && *index <= consumer)
    {
        return None;
    }
    let Instruction::Move { dst, src: local } = function.instructions[producer] else {
        return None;
    };
    if dst != temp || local.index() >= locals || exposed.contains(&local) {
        return None;
    }
    // Straight line from the copy to the consumer: no jump in or out, no
    // branch target, no exception-handler entry, and nothing that can write
    // the local in between.
    for index in producer + 1..=consumer {
        if boundaries.contains(&index)
            || handler_entries.contains(&index)
            || diverts(function.instructions[index])
        {
            return None;
        }
        let instruction = function.instructions[index];
        if operand_regs(function, instruction).writes(local)
            || (runs_php_code(instruction) && function.dynamic_scope)
        {
            return None;
        }
    }
    Some(Candidate {
        consumer,
        path,
        step,
        temp,
        local,
        producer,
    })
}

/// The locals of `function` that may hold a PHP reference at run time.
fn reference_locals(function: &Function) -> HashSet<Reg> {
    let locals = function.locals.len();
    let mut exposed = HashSet::new();
    let register = |index: usize| u16::try_from(index).ok().map(Reg);
    if function.dynamic_scope {
        // `$$name`, `extract` and friends can bind any register to a reference.
        for index in 0..locals {
            if let Some(register) = register(index) {
                exposed.insert(register);
            }
        }
        return exposed;
    }
    for (index, parameter) in function.parameters.iter().enumerate() {
        if parameter.by_reference
            && let Some(reg) = register(index)
        {
            exposed.insert(reg);
        }
    }
    // A by-reference capture is only visible in the parent's `Capture`
    // instructions, so every capture register counts.
    let first_capture = function.parameters.len();
    for index in first_capture..first_capture + function.captures as usize {
        if let Some(reg) = register(index) {
            exposed.insert(reg);
        }
    }
    for instruction in function.instructions.iter() {
        match *instruction {
            Instruction::BindGlobal { local, .. }
            | Instruction::BindStatic { local, .. }
            | Instruction::InitStatic { local, .. }
            | Instruction::SendLocal { local } => {
                exposed.insert(local);
            }
            Instruction::SendPath { path }
            | Instruction::MakeReference { path, .. }
            | Instruction::BindReference { path, .. }
            | Instruction::IterStartReference { path, .. } => {
                if let Some(path) = function.paths.get(path.index())
                    && let PathBase::Register(root) = path.base
                {
                    exposed.insert(root);
                }
            }
            Instruction::CallDirect { args, count, .. }
            | Instruction::CallNativeDirect { args, count, .. } => {
                // A direct call writes by-reference arguments back into the area.
                for offset in 0..count {
                    if let Some(reg) = register(args.index() + offset as usize)
                        && reg.index() < locals
                    {
                        exposed.insert(reg);
                    }
                }
            }
            _ => {}
        }
    }
    exposed
}

/// Whether an instruction can transfer control instead of falling through.
fn diverts(instruction: Instruction) -> bool {
    if target(&instruction).is_some() {
        return true;
    }
    matches!(
        instruction,
        Instruction::FinallyCall { .. }
            | Instruction::FinallyReturn { .. }
            | Instruction::FinallyDiscard { .. }
            | Instruction::Return { .. }
            | Instruction::ReturnNull
            | Instruction::ReturnMissing
            | Instruction::ReturnReference { .. }
            | Instruction::Throw { .. }
            | Instruction::Exit { .. }
    )
}

/// Remove the `Nop`s this pass left behind and remap every target and
/// handler range, as `optimize`'s own compaction does.
fn compact(function: &mut Function) {
    let mut offsets = Vec::with_capacity(function.instructions.len() + 1);
    let mut count = 0;
    for instruction in &function.instructions {
        offsets.push(count);
        if !matches!(instruction, Instruction::Nop) {
            count += 1;
        }
    }
    offsets.push(count);
    let mut lines = Vec::new();
    let mut instructions = Vec::new();
    for (index, mut instruction) in function.instructions.iter().copied().enumerate() {
        if matches!(instruction, Instruction::Nop) {
            continue;
        }
        if let Some(target) = target_mut(&mut instruction) {
            target.0 = offsets.get(target.index()).copied().unwrap_or(target.0);
        }
        // `FinallyCall`'s target is not part of `optimize::target_mut`.
        if let Instruction::FinallyCall { target, .. } = &mut instruction {
            target.0 = offsets.get(target.index()).copied().unwrap_or(target.0);
        }
        instructions.push(instruction);
        lines.push(function.lines.get(index).copied().unwrap_or(1));
    }
    function.instructions = instructions.into();
    function.lines = lines;
    for handler in &mut function.handlers {
        handler.start.0 = offsets[handler.start.index()];
        handler.end.0 = offsets[handler.end.index()];
        if let Some(target) = &mut handler.catch {
            target.0 = offsets[target.index()];
        }
        if let Some(target) = &mut handler.finally {
            target.0 = offsets[target.index()];
        }
    }
}
