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
                elseifs,
                otherwise,
            } => {
                return self.if_statement(condition, then, elseifs, otherwise.as_deref(), scope);
            }
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
}

/// A label can re-enter a block after an otherwise terminal statement.
pub(super) fn contains_label(statement: &Statement) -> bool {
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
