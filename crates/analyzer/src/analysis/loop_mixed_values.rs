//! In `while ($again) { echo strlen($text); $text = $callback($text); }`
//! an untyped callback can contribute a mixed value to the next iteration.

use super::*;

impl Analyzer<'_> {
    pub(super) fn loop_mixed_entry(&self, mut entry: Scope, backedge: &Scope) -> Option<Scope> {
        let mut changed = false;
        for (name, variable) in &backedge.variables {
            if variable.definition == Definition::Definite
                && self.strict_mixed(&variable.ty)
                && entry.get(name).definition == Definition::Definite
                && entry.get(name).ty == ValueType::Mixed
            {
                entry.define(name, ValueType::ImplicitMixed);
                changed = true;
            }
        }
        changed.then_some(entry)
    }

    pub(super) fn check_loop_mixed_body(
        &mut self,
        condition: &Expression,
        body: &[Statement],
        entry: Scope,
    ) {
        // The widened scope already covers the loop's effects. Recheck the
        // body with the new mixed provenance without adding another exit.
        let previous = self.context.clone();
        let exceptions = std::mem::take(&mut self.exception_scopes);
        let returns = std::mem::take(&mut self.return_scopes);
        self.checking_loop_mixed_values = true;
        if let Some(mut entered) = self.condition(condition, entry, false).yes {
            self.control_body(body, &mut entered);
        }
        self.checking_loop_mixed_values = false;
        self.context = previous;
        self.exception_scopes = exceptions;
        self.return_scopes = returns;
    }
}
