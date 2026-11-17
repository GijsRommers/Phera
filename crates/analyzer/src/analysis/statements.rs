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
            scope.dynamic = true;
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
            StatementKind::Namespace {
                name, statements, ..
            } => {
                let previous = self.context.names.clone();
                let namespace = name.as_ref().map_or(String::new(), |n| n.text.clone());
                self.context.names = Names::for_block(namespace, statements);
                self.block(statements, scope);
                self.context.names = previous;
            }
            StatementKind::Function(function) => self.function(function, statement.span),
            StatementKind::Class(class) => self.class_declaration(class),
            StatementKind::Block(body) => return self.block(body, scope),
            StatementKind::Expression(expression) => {
                if self.expression(expression, scope, false) == ValueType::Never {
                    return Flow::Terminal;
                }
            }
            StatementKind::Return(value) => {
                if let Some(value) = value {
                    self.expression(value, scope, false);
                    if self.context.returns == ValueType::Void {
                        self.report(Finding::VoidReturn, statement.span);
                    }
                } else if !matches!(self.context.returns, ValueType::Void | ValueType::Mixed) {
                    self.report(
                        Finding::MissingReturn(self.context.function.clone()),
                        statement.span,
                    );
                }
                return Flow::Terminal;
            }
            StatementKind::If {
                condition,
                then,
                otherwise,
            } => {
                return self.if_statement(condition, then, otherwise.as_deref(), scope);
            }
            StatementKind::Switch { subject, cases } => {
                self.expression(subject, scope, false);
                return self.switch(cases, scope);
            }
            StatementKind::Try {
                body,
                catches,
                finally,
            } => {
                return self.try_statement(body, catches, finally.as_deref(), scope);
            }
            StatementKind::While { condition, body } => {
                self.expression(condition, scope, false);
                return self.loop_body(body, Some(condition), scope, false);
            }
            StatementKind::DoWhile { body, condition } => {
                return self.loop_body(body, Some(condition), scope, true);
            }
            StatementKind::For {
                init,
                condition,
                step,
                body,
            } => {
                for e in init {
                    self.expression(e, scope, false);
                }
                for e in condition {
                    self.expression(e, scope, false);
                }
                let before = scope.clone();
                let flow = self.block(body, scope);
                for e in step {
                    self.expression(e, scope, false);
                }
                *scope = Scope::join(&[before, scope.clone()]);
                if condition.is_empty() && flow != Flow::Break {
                    return Flow::Terminal;
                }
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
                self.block(body, scope);
                *scope = Scope::join(&[before, scope.clone()]);
            }
            StatementKind::Global(variables) => {
                for variable in variables {
                    match variable {
                        Variable::Named(name) => scope.define(name, ValueType::Mixed),
                        Variable::Dynamic(e) => {
                            self.expression(e, scope, false);
                            scope.dynamic = true;
                        }
                    }
                }
            }
            StatementKind::Static(variables) => {
                for variable in variables {
                    let ty = variable
                        .default
                        .as_ref()
                        .map_or(ValueType::Mixed, |e| self.expression(e, scope, false));
                    scope.define(&variable.name, ty);
                }
            }
            StatementKind::Unset(expressions) => {
                for e in expressions {
                    self.expression(e, scope, true);
                    if let ExpressionKind::Variable(Variable::Named(name)) = &e.kind {
                        scope.variables.remove(name);
                        scope.aliases.remove(name);
                    }
                }
            }
            StatementKind::Break(_) => return Flow::Break,
            StatementKind::Continue(_) => return Flow::Continue,
            StatementKind::Declare {
                body: Some(body), ..
            } => return self.block(body, scope),
            StatementKind::Goto(_) | StatementKind::Label(_) => scope.dynamic = true,
            _ => {
                for e in crate::walk::statement_expressions(statement) {
                    self.expression(e, scope, false);
                }
            }
        }
        Flow::Next
    }

    fn if_statement(
        &mut self,
        condition: &Expression,
        then: &[Statement],
        otherwise: Option<&[Statement]>,
        scope: &mut Scope,
    ) -> Flow {
        self.expression(condition, scope, false);
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

    fn loop_body(
        &mut self,
        body: &[Statement],
        condition: Option<&Expression>,
        scope: &mut Scope,
        at_least_once: bool,
    ) -> Flow {
        let before = scope.clone();
        if !at_least_once && let Some(condition) = condition {
            self.refine(condition, scope, true);
        }
        let flow = self.block(body, scope);
        if at_least_once && flow == Flow::Terminal {
            return Flow::Terminal;
        }
        if let Some(condition) = condition {
            self.expression(condition, scope, false);
        }
        if !at_least_once {
            *scope = Scope::join(&[before, scope.clone()]);
        }
        if condition.is_some_and(always_true) && flow != Flow::Break {
            Flow::Terminal
        } else {
            Flow::Next
        }
    }

    fn switch(&mut self, cases: &[SwitchCase], scope: &mut Scope) -> Flow {
        let before = scope.clone();
        let mut paths = Vec::new();
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
            if flow != Flow::Terminal {
                paths.push(branch);
            }
        }
        if paths.is_empty() && !cases.is_empty() {
            Flow::Terminal
        } else {
            *scope = Scope::join(&paths);
            Flow::Next
        }
    }

    fn try_statement(
        &mut self,
        body: &[Statement],
        catches: &[Catch],
        finally: Option<&[Statement]>,
        scope: &mut Scope,
    ) -> Flow {
        let before = scope.clone();
        let body_flow = self.block(body, scope);
        let caught = Scope::join(&[before, scope.clone()]);
        let mut live = Vec::new();
        let mut all = vec![scope.clone()];
        if body_flow == Flow::Next {
            live.push(scope.clone());
        }
        for catch in catches {
            let mut branch = caught.clone();
            for ty in &catch.types {
                self.class_name(ty, true);
            }
            if let Some(name) = &catch.variable {
                let ty = catch
                    .types
                    .first()
                    .and_then(|t| self.class_name(t, false))
                    .map_or(ValueType::Mixed, ValueType::Class);
                branch.define(name, ty);
            }
            let flow = self.block(&catch.body, &mut branch);
            if flow == Flow::Next {
                live.push(branch.clone());
            }
            all.push(branch);
        }
        if let Some(finally) = finally {
            let mut final_scope = Scope::join(&all);
            let flow = self.block(finally, &mut final_scope);
            if flow != Flow::Next {
                *scope = final_scope;
                return flow;
            }
            if !live.is_empty() {
                // Apply definite writes from finally to every continuing path.
                let joined = Scope::join(&all);
                for (name, variable) in &final_scope.variables {
                    if variable.definition == Definition::Definite
                        && joined.get(name).definition != Definition::Definite
                    {
                        for path in &mut live {
                            path.define(name, variable.ty.clone());
                        }
                    }
                }
            }
        }
        if live.is_empty() {
            Flow::Terminal
        } else {
            *scope = Scope::join(&live);
            Flow::Next
        }
    }
}

fn always_true(e: &Expression) -> bool {
    matches!(
        e.kind,
        ExpressionKind::Literal(Literal::Bool(true) | Literal::Int(1))
    )
}
