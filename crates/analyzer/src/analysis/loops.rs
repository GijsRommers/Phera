//! Loop scopes: `do { if ($stop) break; $x = 1; } while ($again)` may omit `$x`.
//! Break/continue paths are collected independently of ordinary body fallthrough.

use super::statements::Flow;
use super::*;

impl Analyzer<'_> {
    pub fn iteration(&mut self, statement: &Statement, scope: &mut Scope) -> Flow {
        match &statement.kind {
            StatementKind::While { condition, body } => {
                let truth = self.condition_truth(condition, scope);
                self.expression(condition, scope, false);
                if truth == Some(false) {
                    return Flow::Next;
                }
                self.loop_body(body, condition, scope, false, truth == Some(true))
            }
            StatementKind::DoWhile { body, condition } => {
                self.loop_body(body, condition, scope, true, true)
            }
            StatementKind::For {
                init,
                condition,
                step,
                body,
            } => {
                for e in init.iter().chain(condition) {
                    self.expression(e, scope, false);
                }
                let entered = condition
                    .last()
                    .and_then(|e| self.condition_truth(e, scope));
                if entered == Some(false) {
                    return Flow::Next;
                }
                self.for_loop(
                    body,
                    step,
                    scope,
                    condition.is_empty(),
                    entered == Some(true),
                )
            }
            StatementKind::Foreach {
                subject,
                key,
                value,
                body,
                ..
            } => {
                self.expression(subject, scope, false);
                let before = scope.clone();
                if let Some(key) = key {
                    self.write(key, ValueType::Mixed, scope);
                }
                self.write(value, ValueType::Mixed, scope);
                let (flow, frame) = self.control_body(body, scope);
                let mut paths = frame.breaks;
                paths.extend(frame.continues);
                if flow == Flow::Next {
                    paths.push(scope.clone());
                }
                let nonempty = matches!(&subject.kind, ExpressionKind::Array(items)
                    if items.iter().any(|i| !i.spread));
                if !nonempty {
                    paths.push(before);
                }
                Self::finish_paths(paths, scope)
            }
            _ => Flow::Next,
        }
    }

    fn for_loop(
        &mut self,
        body: &[Statement],
        step: &[Expression],
        scope: &mut Scope,
        infinite: bool,
        entered: bool,
    ) -> Flow {
        let before = scope.clone();
        let (flow, frame) = self.control_body(body, scope);
        let mut advancing = frame.continues;
        if flow == Flow::Next {
            advancing.push(scope.clone());
        }
        let mut paths = frame.breaks;
        if !advancing.is_empty() {
            let mut next = Scope::join(&advancing);
            for e in step {
                self.expression(e, &mut next, false);
            }
            if !infinite {
                paths.push(next);
            }
        }
        if !entered && !infinite {
            paths.push(before);
        }
        Self::finish_paths(paths, scope)
    }

    fn loop_body(
        &mut self,
        body: &[Statement],
        condition: &Expression,
        scope: &mut Scope,
        post_condition: bool,
        entered: bool,
    ) -> Flow {
        let before = scope.clone();
        if !post_condition {
            self.refine(condition, scope, true);
        }
        let (flow, frame) = self.control_body(body, scope);
        let mut advancing = frame.continues;
        if flow == Flow::Next {
            advancing.push(scope.clone());
        }
        let mut paths = frame.breaks;
        if !advancing.is_empty() {
            let mut next = Scope::join(&advancing);
            if post_condition {
                self.expression(condition, &mut next, false);
            }
            if !always_true(condition) {
                paths.push(next);
            }
        }
        if !entered {
            paths.push(before);
        }
        Self::finish_paths(paths, scope)
    }
}

fn always_true(expression: &Expression) -> bool {
    matches!(
        expression.kind,
        ExpressionKind::Literal(Literal::Bool(true) | Literal::Int(1))
    )
}
