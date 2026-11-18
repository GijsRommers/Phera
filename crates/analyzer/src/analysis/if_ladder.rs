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
//! nesting is not followed by recursion. The ladder is walked down once,
//! analysing each condition and branch, and the results are then combined
//! from the last branch back up to the first, exactly as the nested analysis
//! would combine them on its way out.

use super::*;
use crate::analysis::statements::Flow;

/// What one rung of the ladder leaves for the combination on the way up.
enum Rung {
    /// The condition was known to be false: the rest of the ladder decides.
    Skipped,
    /// The condition was not known: the branch ran on `taken`, and the rest
    /// of the ladder on the scope refined by the condition being false.
    Split {
        taken: Scope,
        flow: Flow,
        /// The scope before the branch, kept when no path falls through.
        before: Scope,
    },
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
        let outer_known = self.context.known.clone();
        let branches = std::iter::once((condition, then))
            .chain(elseifs.iter().map(|e| (&e.condition, &e.body[..])));

        let mut rungs = Vec::new();
        let mut current = std::mem::take(scope);
        let mut bottom = None;
        for (index, (condition, body)) in branches.enumerate() {
            if index > 0 {
                // A nested `if` statement sees the facts of the scope it runs in.
                self.context.known.extend(current.known.iter().cloned());
            }
            let truth = self.condition_truth(condition, &current);
            if self.expression(condition, &mut current, false) == ValueType::Never {
                bottom = Some(Flow::Terminal);
                break;
            }
            match truth {
                Some(true) => {
                    bottom = Some(self.block(body, &mut current));
                    break;
                }
                Some(false) => rungs.push(Rung::Skipped),
                None => {
                    let mut taken = current.clone();
                    self.refine(condition, &mut taken, true);
                    let before = current.clone();
                    self.refine(condition, &mut current, false);
                    let flow = self.block(body, &mut taken);
                    rungs.push(Rung::Split {
                        taken,
                        flow,
                        before,
                    });
                }
            }
        }
        let mut flow = bottom
            .unwrap_or_else(|| otherwise.map_or(Flow::Next, |body| self.block(body, &mut current)));
        self.context.known = outer_known;

        for rung in rungs.into_iter().rev() {
            let Rung::Split {
                taken,
                flow: taken_flow,
                before,
            } = rung
            else {
                continue;
            };
            let (rest, rest_flow) = (current, flow);
            let mut paths = Vec::new();
            if taken_flow == Flow::Next {
                paths.push(taken);
            }
            if rest_flow == Flow::Next {
                paths.push(rest);
            }
            (current, flow) = if !paths.is_empty() {
                (Scope::join(&paths), Flow::Next)
            } else if taken_flow == Flow::Terminal && rest_flow == Flow::Terminal {
                (before, Flow::Terminal)
            } else if taken_flow == Flow::Break || rest_flow == Flow::Break {
                (before, Flow::Break)
            } else {
                (before, Flow::Continue)
            };
        }
        *scope = current;
        flow
    }
}
