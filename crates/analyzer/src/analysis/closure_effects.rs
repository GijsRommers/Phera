//! A callback can change `$this->result` when passed to a function such as `ob_start()`.
//! Remember explicit writes to captured objects so callers can discard stale facts.

use super::*;
use crate::scope::Place;

pub fn captured_writes(closure: &Closure) -> Vec<Place> {
    let mut pending = vec![closure.body.as_slice()];
    let mut writes = std::collections::BTreeSet::new();
    while let Some(block) = pending.pop() {
        for statement in block {
            pending.extend(crate::collect::child_blocks(statement));
            let mut expressions = crate::walk::statement_expressions(statement);
            while let Some(expression) = expressions.pop() {
                if let ExpressionKind::Assign { target, .. }
                | ExpressionKind::AssignReference { target, .. }
                | ExpressionKind::CompoundAssign { target, .. }
                | ExpressionKind::IncDec { target, .. } = &expression.kind
                    && let Some(place) = Place::of(target)
                    && !place.members.is_empty()
                    && ((!closure.is_static && place.root == "this")
                        || closure
                            .uses
                            .iter()
                            .any(|capture| capture.name == place.root))
                {
                    writes.insert(place);
                }
                expressions.extend(crate::walk::children(expression));
            }
        }
    }
    writes.into_iter().collect()
}
