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
            flow = self.statement(statement, scope);
            if flow != Flow::Next {
                break;
            }
        }
        self.depth -= 1;
        flow
    }

    fn statement(&mut self, statement: &Statement, scope: &mut Scope) -> Flow {
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
                self.expression(subject, scope, false);
                return self.switch(cases, scope);
            }
            StatementKind::Try {
                body,
                catches,
                finally,
            } => return self.try_statement(body, catches, finally.as_deref(), scope),
            StatementKind::Break(levels) => return self.control_exit(*levels, false, scope),
            StatementKind::Continue(levels) => return self.control_exit(*levels, true, scope),
            StatementKind::Goto(_) | StatementKind::Label(_) => scope.dynamic = true,
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
            StatementKind::Function(function) => self.function(function, statement.span),
            StatementKind::Class(class) => self.class_declaration(class),
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
        self.expression(condition, scope, false);
        if let Some(truth) = truth {
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

    fn switch(&mut self, cases: &[SwitchCase], scope: &mut Scope) -> Flow {
        use super::control::{ControlFrame, ControlKind};
        let before = scope.clone();
        let mut paths = Vec::new();
        self.controls.push(ControlFrame::new(ControlKind::Switch));
        if !cases.iter().any(|c| c.condition.is_none()) {
            paths.push(before.clone());
        }
        for (start, case) in cases.iter().enumerate() {
            let mut branch = before.clone();
            if let Some(condition) = &case.condition {
                self.expression(condition, &mut branch, false);
            }
            let mut flow = Flow::Next;
            for case in &cases[start..] {
                flow = self.block(&case.body, &mut branch);
                if flow != Flow::Next {
                    break;
                }
            }
            if flow == Flow::Next {
                paths.push(branch);
            }
        }
        if let Some(frame) = self.controls.pop() {
            paths.extend(frame.breaks);
        }
        Self::finish_paths(paths, scope)
    }
}
