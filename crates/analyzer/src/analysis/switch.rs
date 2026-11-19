//! Switch scopes merge matching and falling-through entries before checking a body.
//! `case 0: $x = 1; case 1: echo $x;` reads a possibly, not certainly, defined variable.

use super::*;
use super::{
    control::{ControlFrame, ControlKind},
    statements::Flow,
};

impl Analyzer<'_> {
    pub fn switch(
        &mut self,
        subject: &Expression,
        cases: &[SwitchCase],
        scope: &mut Scope,
    ) -> Flow {
        let value = crate::constants::value(subject, scope);
        if self.expression(subject, scope, false) == ValueType::Never {
            return Flow::Terminal;
        }
        let (mut entries, remaining) = self.switch_entries(cases, value.as_ref(), scope);
        self.controls.push(ControlFrame::new(ControlKind::Switch));
        let mut falling = None;
        for (case, entry) in cases.iter().zip(&mut entries) {
            let mut incoming = entry.take().into_iter().collect::<Vec<_>>();
            incoming.extend(falling.take());
            if incoming.is_empty() {
                continue;
            }
            let mut branch = Scope::join(&incoming);
            if self.block(&case.body, &mut branch) == Flow::Next {
                falling = Some(branch);
            }
        }
        let mut paths: Vec<_> = falling.into_iter().chain(remaining).collect();
        if let Some(frame) = self.controls.pop() {
            paths.extend(frame.breaks);
        }
        Self::finish_paths(paths, scope)
    }

    fn switch_entries(
        &mut self,
        cases: &[SwitchCase],
        subject: Option<&crate::constants::ConstantValue>,
        scope: &Scope,
    ) -> (Vec<Option<Scope>>, Option<Scope>) {
        let mut remaining = Some(scope.clone());
        let mut entries = vec![None; cases.len()];
        for (index, case) in cases.iter().enumerate() {
            let Some(condition) = &case.condition else {
                continue;
            };
            let Some(mut branch) = remaining.take() else {
                break;
            };
            if self.level >= 2
                && let Some(crate::constants::ConstantValue::Bool(truth)) = subject
            {
                // PHP compares switch cases loosely. With a boolean subject
                // the comparison is the truthiness of the case expression.
                let branches = self.condition(condition, branch, false);
                (entries[index], remaining) = if *truth {
                    (branches.yes, branches.no)
                } else {
                    (branches.no, branches.yes)
                };
                continue;
            }
            let value = crate::constants::value(condition, &branch);
            if self.expression(condition, &mut branch, false) == ValueType::Never {
                break;
            }
            let matches = subject.zip(value.as_ref()).and_then(|(a, b)| {
                (std::mem::discriminant(a) == std::mem::discriminant(b)).then_some(a == b)
            });
            if matches != Some(false) {
                entries[index] = Some(branch.clone());
            }
            if matches != Some(true) {
                remaining = Some(branch);
            }
        }
        if let Some(default) = cases.iter().position(|case| case.condition.is_none()) {
            entries[default] = remaining.take();
        }
        (entries, remaining)
    }
}
