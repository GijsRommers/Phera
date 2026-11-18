//! Control flow: `if ($ok) { return 1; } else { throw new Exception; }` cannot fall through.
//! Only continuing branches contribute to the scope after a conditional.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Next,
    Terminal,
    Break,
    Continue,
}

impl Analyzer<'_> {
    pub fn block(&mut self, statements: &[Statement], scope: &mut Scope) -> Flow {
        if self.depth >= MAX_DEPTH {
            self.context.incomplete = true;
            scope.unknown_writes();
            return Flow::Next;
        }
        self.depth += 1;
        let mut flow = Flow::Next;
        for statement in statements {
            if flow != Flow::Next {
                if !contains_label(statement) {
                    continue;
                }
                scope.unknown_writes();
            }
            flow = self.statement(statement, scope);
        }
        self.depth -= 1;
        flow
    }

    fn statement(&mut self, statement: &Statement, scope: &mut Scope) -> Flow {
        let previous = self.context.known.clone();
        self.context.known.extend(scope.known.iter().cloned());
        let flow = self.statement_kind(statement, scope);
        self.context.known = previous;
        flow
    }

    fn statement_kind(&mut self, statement: &Statement, scope: &mut Scope) -> Flow {
        match &statement.kind {
            StatementKind::Namespace { .. }
            | StatementKind::Function(_)
            | StatementKind::Class(_) => return self.declaration_statement(statement, scope),
            StatementKind::While { .. }
            | StatementKind::DoWhile { .. }
            | StatementKind::For { .. }
            | StatementKind::Foreach { .. } => return self.iteration(statement, scope),
            StatementKind::Global(_) | StatementKind::Static(_) | StatementKind::Unset(_) => {
                self.binding_statement(statement, scope)
            }
            StatementKind::Block(body)
            | StatementKind::Declare {
                body: Some(body), ..
            } => return self.block(body, scope),
            StatementKind::Expression(expression) => {
                if self.expression(expression, scope, false) == ValueType::Never {
                    return Flow::Terminal;
                }
            }
            StatementKind::Return(value) => {
                return self.return_statement(value.as_ref(), statement.span, scope);
            }
            StatementKind::If {
                condition,
                then,
                otherwise,
            } => return self.if_statement(condition, then, otherwise.as_deref(), scope),
            StatementKind::Switch { subject, cases } => {
                return self.switch(subject, cases, scope);
            }
            StatementKind::Try {
                body,
                catches,
                finally,
            } => return self.try_statement(body, catches, finally.as_deref(), scope),
            StatementKind::Break(levels) => return self.control_exit(*levels, false, scope),
            StatementKind::Continue(levels) => return self.control_exit(*levels, true, scope),
            // Incoming jumps can bypass assignments or revisit a label.
            StatementKind::Goto(_) | StatementKind::Label(_) => scope.unknown_writes(),
            _ => {
                for e in crate::walk::statement_expressions(statement) {
                    self.expression(e, scope, false);
                }
            }
        }
        Flow::Next
    }

    fn declaration_statement(&mut self, statement: &Statement, scope: &mut Scope) -> Flow {
        match &statement.kind {
            StatementKind::Namespace {
                name, statements, ..
            } => {
                let previous = self.context.names.clone();
                let namespace = name.as_ref().map_or(String::new(), |n| n.text.clone());
                self.context.names = Names::for_block(namespace, statements);
                let flow = self.block(statements, scope);
                self.context.names = previous;
                return flow;
            }
            StatementKind::Function(function) => {
                let previous = self.context.known.clone();
                self.context.known.extend(scope.known.iter().cloned());
                self.function(function, statement.span);
                self.context.known = previous;
            }
            StatementKind::Class(class) => {
                let previous = self.context.known.clone();
                self.context.known.extend(scope.known.iter().cloned());
                self.class_declaration(class);
                self.context.known = previous;
            }
            _ => {}
        }
        Flow::Next
    }

    fn return_statement(
        &mut self,
        value: Option<&Expression>,
        span: Span,
        scope: &mut Scope,
    ) -> Flow {
        if let Some(value) = value {
            if self.context.by_reference && matches!(value.kind, ExpressionKind::Variable(_)) {
                self.ensure_reference(value, scope);
            }
            self.expression(value, scope, false);
            if self.context.returns == ValueType::Void {
                self.report(Finding::VoidReturn, span);
            }
        } else if self.context.declared_return && self.context.returns != ValueType::Void {
            self.report(Finding::MissingReturn(self.context.function.clone()), span);
        }
        self.record_return(scope);
        Flow::Terminal
    }

    fn if_statement(
        &mut self,
        condition: &Expression,
        then: &[Statement],
        otherwise: Option<&[Statement]>,
        scope: &mut Scope,
    ) -> Flow {
        let truth = self.condition_truth(condition, scope);
        if self.expression(condition, scope, false) == ValueType::Never {
            return Flow::Terminal;
        }
        if let Some(truth) = truth
            && !(if truth {
                otherwise.is_some_and(|body| body.iter().any(contains_label))
            } else {
                then.iter().any(contains_label)
            })
        {
            return if truth {
                self.block(then, scope)
            } else {
                otherwise.map_or(Flow::Next, |body| self.block(body, scope))
            };
        }
        let mut left = scope.clone();
        let mut right = scope.clone();
        self.refine(condition, &mut left, true);
        self.refine(condition, &mut right, false);
        let a = self.block(then, &mut left);
        let b = otherwise.map_or(Flow::Next, |body| self.block(body, &mut right));
        let mut paths = Vec::new();
        if a == Flow::Next {
            paths.push(left);
        }
        if b == Flow::Next {
            paths.push(right);
        }
        if !paths.is_empty() {
            *scope = Scope::join(&paths);
            return Flow::Next;
        }
        if a == Flow::Terminal && b == Flow::Terminal {
            Flow::Terminal
        } else if a == Flow::Break || b == Flow::Break {
            Flow::Break
        } else {
            Flow::Continue
        }
    }
}

/// A label can re-enter a block after an otherwise terminal statement.
fn contains_label(statement: &Statement) -> bool {
    let mut pending = vec![statement];
    while let Some(statement) = pending.pop() {
        if matches!(statement.kind, StatementKind::Label(_)) {
            return true;
        }
        pending.extend(
            crate::collect::child_blocks(statement)
                .into_iter()
                .flatten(),
        );
        if let StatementKind::Namespace { statements, .. } = &statement.kind {
            pending.extend(statements);
        }
    }
    false
}
