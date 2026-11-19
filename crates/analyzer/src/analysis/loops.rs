//! Loop scopes: `do { if ($stop) break; $x = 1; } while ($again)` may omit `$x`.
//! Break/continue paths are collected independently of ordinary body fallthrough.

use super::statements::Flow;
use super::*;

impl Analyzer<'_> {
    pub fn iteration(&mut self, statement: &Statement, scope: &mut Scope) -> Flow {
        match &statement.kind {
            StatementKind::While { condition, body } => {
                let branches = self.condition(condition, std::mem::take(scope), false);
                let Some(entered) = branches.yes else {
                    return Self::finish_paths(branches.no.into_iter().collect(), scope);
                };
                *scope = entered;
                self.loop_body(body, condition, scope, branches.no)
            }
            StatementKind::DoWhile { body, condition } => {
                self.loop_body(body, condition, scope, None)
            }
            StatementKind::For {
                init,
                condition,
                step,
                body,
            } => {
                let (last, earlier) = condition
                    .split_last()
                    .map_or((None, &[][..]), |(last, earlier)| (Some(last), earlier));
                for expression in init.iter().chain(earlier) {
                    if self.expression(expression, scope, false) == ValueType::Never {
                        return Flow::Terminal;
                    }
                }
                let Some(last) = last else {
                    return self.for_loop(body, step, scope, None, true);
                };
                let branches = self.condition(last, std::mem::take(scope), false);
                if let Some(entered) = branches.yes {
                    *scope = entered;
                    self.for_loop(body, step, scope, branches.no, always_true(last))
                } else {
                    Self::finish_paths(branches.no.into_iter().collect(), scope)
                }
            }
            StatementKind::Foreach {
                subject,
                key,
                value,
                body,
                by_reference,
            } => self.foreach_loop(subject, key.as_ref(), value, *by_reference, body, scope),
            _ => Flow::Next,
        }
    }

    fn foreach_loop(
        &mut self,
        subject: &Expression,
        key: Option<&Expression>,
        value: &Expression,
        by_reference: bool,
        body: &[Statement],
        scope: &mut Scope,
    ) -> Flow {
        let subject_type = self.expression(subject, scope, false);
        if subject_type == ValueType::Never {
            return Flow::Terminal;
        }
        let before = scope.clone();
        let nonempty = matches!(&subject.kind, ExpressionKind::Array(items)
            if items.iter().any(|item| !item.spread));
        let (key_type, value_type) = if self.level >= 2 && !by_reference {
            super::arrays::iteration_types(&subject_type)
        } else {
            (ValueType::Mixed, ValueType::Mixed)
        };
        let key_flow = key.map_or(Evaluation::Completes, |key| {
            self.write(key, key_type, scope)
        });
        if key_flow == Evaluation::Never
            || self.write(value, value_type, scope) == Evaluation::Never
        {
            *scope = before;
            return if nonempty { Flow::Terminal } else { Flow::Next };
        }
        if by_reference {
            self.external_reference(value, scope);
        }
        let (flow, frame) = self.control_body(body, scope);
        let mut paths = frame.breaks;
        paths.extend(frame.continues);
        if flow == Flow::Next {
            paths.push(scope.clone());
        }
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
        bypass: Option<Scope>,
        infinite: bool,
    ) -> Flow {
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
        paths.extend(bypass);
        Self::finish_paths(paths, scope)
    }

    fn loop_body(
        &mut self,
        body: &[Statement],
        condition: &Expression,
        scope: &mut Scope,
        bypass: Option<Scope>,
    ) -> Flow {
        let (flow, frame) = self.control_body(body, scope);
        let mut advancing = frame.continues;
        if flow == Flow::Next {
            advancing.push(scope.clone());
        }
        let mut paths = frame.breaks;
        if !advancing.is_empty() {
            let next = Scope::join(&advancing);
            let branches = self.condition(condition, next, false);
            paths.extend(branches.no);
            if !always_true(condition) {
                // A later iteration can revisit the condition after changing
                // a variable that made it true on the first pass.
                paths.extend(branches.yes);
            }
        }
        paths.extend(bypass);
        Self::finish_paths(paths, scope)
    }
}

fn always_true(expression: &Expression) -> bool {
    matches!(
        expression.kind,
        ExpressionKind::Literal(Literal::Bool(true))
    ) || matches!(expression.kind, ExpressionKind::Literal(Literal::Int(n)) if n != 0)
}
