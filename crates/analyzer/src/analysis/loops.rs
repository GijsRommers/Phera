//! Loop scopes: `do { if ($stop) break; $x = 1; } while ($again)` may omit `$x`.
//! Break/continue paths are collected independently of ordinary body fallthrough.

use super::statements::Flow;
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConditionPosition {
    BeforeBody,
    AfterBody,
}

impl Analyzer<'_> {
    pub fn iteration(&mut self, statement: &Statement, scope: &mut Scope) -> Flow {
        match &statement.kind {
            StatementKind::While { condition, body } => {
                self.widen_loop(body, [condition], scope);
                let branches = self.condition(condition, std::mem::take(scope), false);
                let Some(entered) = branches.yes else {
                    self.constant_condition("while", "While loop condition", condition, &branches);
                    return Self::finish_paths(branches.no.into_iter().collect(), scope);
                };
                *scope = entered;
                self.loop_body(
                    body,
                    condition,
                    scope,
                    branches.no,
                    ConditionPosition::BeforeBody,
                )
            }
            StatementKind::DoWhile { body, condition } => {
                self.widen_loop(body, [condition], scope);
                self.loop_body(body, condition, scope, None, ConditionPosition::AfterBody)
            }
            StatementKind::For {
                init,
                condition,
                step,
                body,
            } => {
                let new_locals = self.loop_initializer_locals(init, scope);
                let flow = self.for_iteration(init, condition, step, body, scope);
                if flow == Flow::Next {
                    for name in new_locals {
                        if let Some(variable) = scope.variables.get_mut(name) {
                            variable.definition = Definition::Possible;
                        }
                        scope.facts.forget_variable(name);
                        scope.values.remove(name);
                    }
                }
                flow
            }
            StatementKind::Foreach {
                subject,
                key,
                value,
                body,
                by_reference,
            } => {
                self.foreach_overwrite(key.as_ref(), value, scope, statement.span);
                self.foreach_loop(subject, key.as_ref(), value, *by_reference, body, scope)
            }
            _ => Flow::Next,
        }
    }

    fn for_iteration(
        &mut self,
        init: &[Expression],
        condition: &[Expression],
        step: &[Expression],
        body: &[Statement],
        scope: &mut Scope,
    ) -> Flow {
        let (last, earlier) = condition
            .split_last()
            .map_or((None, &[][..]), |(last, earlier)| (Some(last), earlier));
        for expression in init.iter().chain(earlier) {
            if self.expression(expression, scope, false) == ValueType::Never {
                return Flow::Terminal;
            }
        }
        let Some(last) = last else {
            self.widen_loop(body, step, scope);
            return self.for_loop(body, step, scope, None, true);
        };
        let definitely_enters = self.level >= 4
            && crate::constants::value(last, scope).is_some_and(|value| value.truth());
        self.widen_loop(body, condition.iter().chain(step), scope);
        let branches = self.condition(last, std::mem::take(scope), false);
        if let Some(entered) = branches.yes {
            *scope = entered;
            let bypass = if definitely_enters { None } else { branches.no };
            self.for_loop(body, step, scope, bypass, always_true(last))
        } else {
            Self::finish_paths(branches.no.into_iter().collect(), scope)
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
        if self.strict_mixed(&subject_type) {
            self.diagnose(
                "foreach.nonIterable", subject.span,
                "Argument of an invalid type mixed supplied for foreach, only iterables are supported.".into(),
            );
        }
        if self.level >= 4
            && matches!(&subject_type, ValueType::Shape(shape) if shape.fields.is_empty()
                && shape.remainder.is_none()
                && shape.kind != crate::types::ShapeKind::Object)
        {
            self.diagnose_dead_code(
                "foreach.emptyArray",
                subject.span,
                "Empty array passed to foreach.".into(),
            );
            return Flow::Next;
        }
        if self.level >= 2 && by_reference {
            // Writes through the element alias can change the array's shape.
            if let ExpressionKind::Variable(Variable::Named(name)) = &subject.kind {
                if matches!(&subject_type, ValueType::Array | ValueType::Shape(_)) {
                    scope.define(name, ValueType::Array);
                } else {
                    scope.define(name, ValueType::Mixed);
                }
            } else {
                scope.forget_member_write(subject);
            }
        }
        let before = scope.clone();
        self.widen_loop(body, [], scope);
        let nonempty = matches!(&subject.kind, ExpressionKind::Array(items)
            if items.iter().any(|item| !item.spread))
            || self.level >= 7 && super::arrays::nonempty_array(&subject_type);
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
        if !nonempty || self.options.strict && !paths.is_empty() {
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
        position: ConditionPosition,
    ) -> Flow {
        let (flow, frame) = self.control_body(body, scope);
        let mut advancing = frame.continues;
        if flow == Flow::Next {
            advancing.push(scope.clone());
        }
        let mut paths = frame.breaks;
        let has_break = !paths.is_empty();
        if !advancing.is_empty() {
            let next = Scope::join(&advancing);
            let branches = self.condition(condition, next, false);
            if position == ConditionPosition::AfterBody
                && (branches.truth() != Some(true) || !has_break && !self.context.generator)
            {
                self.constant_condition("doWhile", "Do-while loop condition", condition, &branches);
            }
            if position == ConditionPosition::BeforeBody
                && !has_break
                && bypass.is_none()
                && branches.truth() == Some(true)
                && self.context.returns != ValueType::Never
                && !has_terminal_exit(body)
                && !super::generators::body_yields(body)
            {
                self.constant_condition("while", "While loop condition", condition, &branches);
            }
            if let Some(exited) = branches.no {
                paths.push(exited);
            } else if !always_true(condition) {
                // A later iteration can revisit a condition that is true
                // on the first pass. Use this fallback only if there is no
                // false exit to carry the condition's effects.
                paths.extend(branches.yes);
            }
        }
        paths.extend(bypass);
        Self::finish_paths(paths, scope)
    }
}

fn has_terminal_exit(body: &[Statement]) -> bool {
    let mut statements: Vec<_> = body.iter().collect();
    while let Some(statement) = statements.pop() {
        if matches!(
            statement.kind,
            StatementKind::Return(_) | StatementKind::Continue(2..)
        ) {
            return true;
        }
        let mut expressions = crate::walk::statement_expressions(statement);
        while let Some(expression) = expressions.pop() {
            if matches!(
                expression.kind,
                ExpressionKind::Throw(_) | ExpressionKind::Exit(_)
            ) {
                return true;
            }
            expressions.extend(crate::walk::children(expression));
        }
        statements.extend(
            crate::collect::child_blocks(statement)
                .into_iter()
                .flatten(),
        );
    }
    false
}

fn always_true(expression: &Expression) -> bool {
    matches!(
        expression.kind,
        ExpressionKind::Literal(Literal::Bool(true))
    ) || matches!(expression.kind, ExpressionKind::Literal(Literal::Int(n)) if n != 0)
}
