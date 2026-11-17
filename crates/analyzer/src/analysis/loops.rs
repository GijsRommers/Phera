//! Loop scopes: `do { if ($stop) break; $x = 1; } while ($again)` may omit `$x`.
//! Break/continue paths are collected independently of ordinary body fallthrough.

use super::statements::Flow;
use super::*;

impl Analyzer<'_> {
    pub fn iteration(&mut self, statement: &Statement, scope: &mut Scope) -> Flow {
        match &statement.kind {
            StatementKind::While { condition, body } => {
                let truth = self.condition_truth(condition, scope);
                if self.expression(condition, scope, false) == ValueType::Never {
                    return Flow::Terminal;
                }
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
                    if self.expression(e, scope, false) == ValueType::Never {
                        return Flow::Terminal;
                    }
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
            } => self.foreach_loop(subject, key.as_ref(), value, body, scope),
            _ => Flow::Next,
        }
    }

    fn foreach_loop(
        &mut self,
        subject: &Expression,
        key: Option<&Expression>,
        value: &Expression,
        body: &[Statement],
        scope: &mut Scope,
    ) -> Flow {
        if self.expression(subject, scope, false) == ValueType::Never {
            return Flow::Terminal;
        }
        let before = scope.clone();
        let nonempty = matches!(&subject.kind, ExpressionKind::Array(items)
            if items.iter().any(|item| !item.spread));
        let key_flow = key.map_or(Evaluation::Completes, |key| {
            self.write(key, ValueType::Mixed, scope)
        });
        if key_flow == Evaluation::Never
            || self.write(value, ValueType::Mixed, scope) == Evaluation::Never
        {
            *scope = before;
            return if nonempty { Flow::Terminal } else { Flow::Next };
        }
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
            let completed = step
                .iter()
                .all(|e| self.expression(e, &mut next, false) != ValueType::Never);
            if completed && !infinite {
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
            let completed = self.expression(condition, &mut next, false) != ValueType::Never;
            if completed && !always_true(condition) {
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
