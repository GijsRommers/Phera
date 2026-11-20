//! `if` ladders: `if ($a) { ... } elseif ($b) { ... } else { ... }`.
//!
//! The ladder is analysed as if each `elseif` were an `if` nested in the
//! `else` of the branch before it, which is what PHP runs:
//!
//! ```php
//! if ($user === null) { return; }      // after the ladder, $user is known
//! elseif (!$user->active) { throw new Exception; }
//! else { $name = $user->name; }        // only this branch falls through
//! ```
//!
//! Generated code holds ladders of a hundred thousand `elseif`s, so the
//! nesting is not followed by recursion. The ladder is walked down once: each
//! rung's condition splits its entry scope into a "yes" and a "no" scope
//! ([`Analyzer::condition`]), the branch runs on "yes", and "no" is where the
//! next rung starts. The rungs' results are then combined from the last one
//! back up to the first with [`condition::Condition::merge`], exactly as the
//! nested analysis combines them on its way out of each `if`.

use super::*;
use crate::analysis::statements::{Flow, contains_label, is_throw};

/// What a rung leaves for the combination on the way up: how its own branch
/// ended, and the scope it left if it falls through.
struct Rung {
    flow: Flow,
    taken: Option<Scope>,
    guard: Option<(crate::scope::GuardKey, bool, usize)>,
}

impl Analyzer<'_> {
    pub(super) fn if_statement(
        &mut self,
        condition: &Expression,
        then: &[Statement],
        elseifs: &[ElseIf],
        otherwise: Option<&[Statement]>,
        scope: &mut Scope,
    ) -> Flow {
        // Each `elseif` is an `if` statement inside a block in the nested
        // form: entering the first one saves the known facts and the pending
        // closure binding, and leaving the ladder restores them.
        let mut saved = None;
        let branches: Vec<(&Expression, &[Statement])> = std::iter::once((condition, then))
            .chain(
                elseifs
                    .iter()
                    .map(|elseif| (&elseif.condition, &elseif.body[..])),
            )
            .collect();
        // Whether the rungs from an index on, or the `else`, hold a label;
        // only needed when a rung's "no" outcome cannot happen.
        let mut rest_holds_label: Option<Vec<bool>> = None;

        let mut rungs = Vec::new();
        let mut entry = scope.clone();
        // How the part of the ladder below the last rung walked ended.
        let bottom = 'walk: {
            for (index, &(condition, body)) in branches.iter().enumerate() {
                if index > 0 {
                    if index == 1 {
                        saved = Some((self.context.known.clone(), self.pending_binding.clone()));
                    }
                    self.context.known.extend(entry.known.iter().cloned());
                }
                let last_condition = self.last_condition;
                if self.level >= 4
                    && !self.options.report_always_true_last_condition
                    && index > 0
                    && index + 1 == branches.len()
                    && otherwise
                        .is_none_or(|body| matches!(body, [statement] if is_throw(statement)))
                {
                    self.last_condition = Some(condition.span);
                }
                let guard = self.enum_guard(condition, &entry).map(|(guard, inverted)| {
                    let offset = condition.span.start;
                    entry.track_guard_root(offset, guard.root());
                    (guard, inverted, offset)
                });
                let outcome = self.condition(condition, entry.clone(), false);
                let (kind, label) = if index == 0 {
                    ("if", "If condition")
                } else {
                    ("elseif", "Elseif condition")
                };
                self.constant_condition(kind, label, condition, &outcome);
                self.last_condition = last_condition;
                let (flow, taken) = self.statement_branch(body, outcome.yes, &entry);
                rungs.push(Rung { flow, taken, guard });
                if index + 1 == branches.len() {
                    break 'walk self.statement_branch(
                        otherwise.unwrap_or_default(),
                        outcome.no,
                        &entry,
                    );
                }
                entry = match outcome.no {
                    Some(no) => no,
                    None => {
                        let holds = rest_holds_label
                            .get_or_insert_with(|| suffix_labels(&branches, otherwise));
                        if !holds[index + 1] {
                            break 'walk (Flow::Terminal, None);
                        }
                        let mut jumped = entry;
                        jumped.unknown_writes();
                        jumped
                    }
                };
            }
            (Flow::Next, None)
        };
        if let Some((known, binding)) = saved {
            self.context.known = known;
            self.pending_binding = binding;
        }

        let (mut flow, mut after) = bottom;
        for rung in rungs.into_iter().rev() {
            let guard_offset = rung.guard.as_ref().map(|(_, _, offset)| *offset);
            let guarded = rung.guard.and_then(|(guard, inverted, offset)| {
                let yes = rung.taken.as_ref()?;
                let no = after.as_ref()?;
                if !yes.guard_root_unchanged(offset) || !no.guard_root_unchanged(offset) {
                    return None;
                }
                let (yes, no) = if inverted { (no, yes) } else { (yes, no) };
                Some((guard, yes.clone(), no.clone()))
            });
            (flow, after) = match condition::Condition::merge(rung.taken, after) {
                Some(mut merged) => {
                    if let Some(offset) = guard_offset {
                        merged.finish_guard(offset);
                    }
                    if let Some((guard, yes, no)) = guarded {
                        merged.remember_definitions(guard, Some(&yes), Some(&no));
                    }
                    (Flow::Next, Some(merged))
                }
                None => (combined(rung.flow, flow), None),
            };
        }
        if let Some(after) = after {
            *scope = after;
        }
        flow
    }
}

/// How an `if` ends when neither branch falls through.
fn combined(taken: Flow, rest: Flow) -> Flow {
    if taken == Flow::Terminal && rest == Flow::Terminal {
        Flow::Terminal
    } else if taken == Flow::Break || rest == Flow::Break {
        Flow::Break
    } else {
        Flow::Continue
    }
}

/// For each rung index, whether that rung or any after it, or the `else`,
/// holds a label; one entry more than there are rungs, for the `else` alone.
fn suffix_labels(
    branches: &[(&Expression, &[Statement])],
    otherwise: Option<&[Statement]>,
) -> Vec<bool> {
    let mut holds = vec![false; branches.len() + 1];
    holds[branches.len()] = otherwise.is_some_and(|body| body.iter().any(contains_label));
    for index in (0..branches.len()).rev() {
        holds[index] = holds[index + 1] || branches[index].1.iter().any(contains_label);
    }
    holds
}
