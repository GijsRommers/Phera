//! Match content particles without a Rust call stack proportional to XML depth.
//!
//! ```php
//! $document->schemaValidateSource($schema); // checks the order of a sequence
//! ```

use crate::tree::{Arena, NodeId};

use super::{Definition, Rule, Schema, elements, expanded};

const MAX_BRANCHES: usize = 4096;
const MAX_TASKS: usize = 1024;

#[derive(Clone, Copy)]
enum Task {
    Particle(Rule),
    Once(Rule),
    Repeat(Rule, usize, usize),
    Progress {
        rule: Rule,
        minimum: usize,
        maximum: usize,
        position: usize,
    },
}

#[derive(Clone, Default)]
struct State {
    tasks: Vec<Task>,
    position: usize,
    match_tail: Option<usize>,
}

struct Matched {
    previous: Option<usize>,
    node: NodeId,
    rule: Rule,
}

enum Child {
    Declared(Rule),
    Skip,
}

pub(super) enum Mismatch {
    Unexpected(NodeId),
    Missing,
    Unsupported,
    Limit,
}

impl Schema {
    pub(super) fn is_particle(&self, rule: Rule) -> bool {
        matches!(
            self.node(rule).name.local.as_str(),
            "element" | "any" | "sequence" | "choice" | "all" | "group"
        )
    }

    pub(super) fn match_content(
        &self,
        arena: &Arena,
        node: NodeId,
        rules: &[Rule],
        work: &mut usize,
    ) -> Result<Vec<(NodeId, Rule)>, Mismatch> {
        let children: Vec<_> = elements(arena, node).collect();
        let mut branches = vec![State {
            tasks: rules.iter().rev().copied().map(Task::Particle).collect(),
            ..State::default()
        }];
        let mut matches = Vec::<Matched>::new();
        let mut furthest = 0;
        'branch: while let Some(mut state) = branches.pop() {
            loop {
                if *work == 0 || branches.len() >= MAX_BRANCHES || state.tasks.len() >= MAX_TASKS {
                    return Err(Mismatch::Limit);
                }
                *work -= 1;
                furthest = furthest.max(state.position);
                let Some(task) = state.tasks.pop() else {
                    if state.position != children.len() {
                        continue 'branch;
                    }
                    let mut result = Vec::new();
                    while let Some(tail) = state.match_tail {
                        let item = &matches[tail];
                        result.push((item.node, item.rule));
                        state.match_tail = item.previous;
                    }
                    return Ok(result);
                };
                match task {
                    Task::Particle(rule) => {
                        let (min, max) = self.occurs(rule).ok_or(Mismatch::Unsupported)?;
                        state.tasks.push(Task::Repeat(rule, min, max));
                    }
                    Task::Repeat(rule, min, max) => {
                        if max == 0 {
                            continue;
                        }
                        if min == 0 {
                            branches.push(state.clone());
                        }
                        state.tasks.push(Task::Progress {
                            rule,
                            minimum: min.saturating_sub(1),
                            maximum: max - 1,
                            position: state.position,
                        });
                        state.tasks.push(Task::Once(rule));
                    }
                    Task::Progress {
                        rule,
                        minimum,
                        maximum,
                        position,
                    } => {
                        if state.position != position || minimum > 0 {
                            state.tasks.push(Task::Repeat(rule, minimum, maximum));
                        }
                    }
                    Task::Once(rule) => match self.node(rule).name.local.as_str() {
                        "sequence" => {
                            self.push_sequence(rule, &mut state);
                        }
                        "choice" => {
                            let choices: Vec<_> = self
                                .children(rule)
                                .filter(|&child| self.is_particle(child))
                                .collect();
                            for choice in choices.into_iter().rev() {
                                let mut alternative = state.clone();
                                alternative.tasks.push(Task::Particle(choice));
                                branches.push(alternative);
                            }
                            continue 'branch;
                        }
                        "group" => {
                            let group = self
                                .declaration(rule, Definition::Group)
                                .ok_or(Mismatch::Unsupported)?;
                            self.push_sequence(group, &mut state);
                        }
                        "all" => {
                            if !self.match_all(rule, arena, &children, &mut state, &mut matches)? {
                                furthest = furthest.max(state.position);
                                continue 'branch;
                            }
                        }
                        "element" | "any" => {
                            let Some(&child) = children.get(state.position) else {
                                continue 'branch;
                            };
                            let Some(matched) = self.match_child(arena, child, rule) else {
                                continue 'branch;
                            };
                            append(&mut state, &mut matches, child, matched);
                        }
                        _ => return Err(Mismatch::Unsupported),
                    },
                }
            }
        }
        Err(children
            .get(furthest)
            .copied()
            .map_or(Mismatch::Missing, Mismatch::Unexpected))
    }

    fn push_sequence(&self, rule: Rule, state: &mut State) {
        let offset = state.tasks.len();
        state.tasks.extend(
            self.children(rule)
                .filter(|&child| self.is_particle(child))
                .map(Task::Particle),
        );
        state.tasks[offset..].reverse();
    }

    fn occurs(&self, rule: Rule) -> Option<(usize, usize)> {
        let minimum = self.attr(rule, "minOccurs").unwrap_or("1").parse().ok()?;
        let maximum = match self.attr(rule, "maxOccurs").unwrap_or("1") {
            "unbounded" => usize::MAX,
            value => value.parse().ok()?,
        };
        (minimum <= maximum).then_some((minimum, maximum))
    }

    fn match_child(&self, arena: &Arena, node: NodeId, rule: Rule) -> Option<Child> {
        let name = &arena[node].name;
        if self.is(rule, "any") {
            if !self.in_namespace(rule, name) {
                return None;
            }
            let process = self.attr(rule, "processContents").unwrap_or("strict");
            if process == "skip" {
                return Some(Child::Skip);
            }
            return self
                .named(Definition::Element, expanded(name))
                .map(Child::Declared)
                .or_else(|| (process == "lax").then_some(Child::Skip));
        }
        let (namespace, local) = self.declared_name(rule, Definition::Element)?;
        if name.local == local && name.namespace.as_deref().unwrap_or_default() == namespace {
            self.declaration(rule, Definition::Element)
                .map(Child::Declared)
        } else {
            None
        }
    }

    fn match_all(
        &self,
        rule: Rule,
        arena: &Arena,
        children: &[NodeId],
        state: &mut State,
        matches: &mut Vec<Matched>,
    ) -> Result<bool, Mismatch> {
        let mut remaining: Vec<_> = self
            .children(rule)
            .filter(|&rule| self.is(rule, "element"))
            .map(|rule| match self.occurs(rule) {
                Some((min, 0..=1)) => Ok((rule, min > 0)),
                _ => Err(Mismatch::Unsupported),
            })
            .collect::<Result<_, _>>()?;
        while let Some(&child) = children.get(state.position) {
            let Some((index, matched)) =
                remaining
                    .iter()
                    .enumerate()
                    .find_map(|(index, &(rule, _))| {
                        self.match_child(arena, child, rule)
                            .map(|matched| (index, matched))
                    })
            else {
                break;
            };
            remaining.swap_remove(index);
            append(state, matches, child, matched);
        }
        Ok(remaining.iter().all(|&(_, required)| !required))
    }
}

fn append(state: &mut State, matches: &mut Vec<Matched>, node: NodeId, child: Child) {
    if let Child::Declared(rule) = child {
        let previous = state.match_tail.replace(matches.len());
        matches.push(Matched {
            previous,
            node,
            rule,
        });
    }
    state.position += 1;
}
