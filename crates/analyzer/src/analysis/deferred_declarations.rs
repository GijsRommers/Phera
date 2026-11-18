//! Declarations outlive statement flow: `return; function f() { missing(); }`.
//! A structural pass checks unvisited bodies, while excluding inactive native polyfills.

use super::*;
use std::collections::BTreeSet;

type DeclarationBlock<'a> = (&'a [Statement], Names, BTreeSet<Symbol>);

impl Analyzer<'_> {
    pub fn deferred_declarations(&mut self) {
        let previous = self.context.clone();
        let statements = &self.file.program.statements;
        let names = Names::for_block(String::new(), statements);
        let mut pending = vec![(&statements[..], names, BTreeSet::new())];
        while let Some((body, names, known)) = pending.pop() {
            self.context.names = names;
            self.context.known = known;
            for statement in body {
                self.deferred_statement(statement, &mut pending);
            }
        }
        self.context = previous;
    }

    fn deferred_statement<'a>(
        &mut self,
        statement: &'a Statement,
        pending: &mut Vec<DeclarationBlock<'a>>,
    ) {
        let names = self.context.names.clone();
        let known = self.context.known.clone();
        match &statement.kind {
            StatementKind::Namespace {
                name, statements, ..
            } => {
                let namespace = name
                    .as_ref()
                    .map_or(String::new(), |name| name.text.clone());
                pending.push((statements, Names::for_block(namespace, statements), known));
            }
            StatementKind::Function(function) => {
                self.function(function, statement.span);
                pending.push((&function.body, names, known));
            }
            StatementKind::Class(class) => {
                self.class_declaration(class);
                self.deferred_members(class, pending);
            }
            StatementKind::If {
                condition,
                then,
                elseifs,
                otherwise,
            } => {
                self.deferred_if(condition, then, elseifs, otherwise.as_deref(), pending);
                return;
            }
            _ => {
                for body in crate::collect::child_blocks(statement) {
                    pending.push((body, names.clone(), known.clone()));
                }
            }
        }
        self.deferred_expressions(statement, pending);
    }

    fn deferred_members<'a>(
        &self,
        class: &'a ClassDeclaration,
        pending: &mut Vec<DeclarationBlock<'a>>,
    ) {
        for member in &class.members {
            if let ClassMemberKind::Method(method) = &member.kind
                && let Some(body) = &method.body
            {
                pending.push((body, self.context.names.clone(), self.context.known.clone()));
            }
        }
    }

    /// An `if` ladder, rung by rung: each `elseif` is reached with what the
    /// conditions before it being false tells, as if it were an `if` nested
    /// in the `else` before it.
    fn deferred_if<'a>(
        &mut self,
        condition: &'a Expression,
        then: &'a [Statement],
        elseifs: &'a [ElseIf],
        otherwise: Option<&'a [Statement]>,
        pending: &mut Vec<DeclarationBlock<'a>>,
    ) {
        let outer_known = self.context.known.clone();
        let names = self.context.names.clone();
        let last = elseifs.len();
        let branches = std::iter::once((condition, then)).chain(
            elseifs
                .iter()
                .map(|elseif| (&elseif.condition, &elseif.body[..])),
        );
        for (index, (condition, body)) in branches.enumerate() {
            let truth = self.condition_truth(condition, &Scope::default());
            if truth != Some(false) {
                let mut scope = Scope::default();
                self.refine(condition, &mut scope, true);
                scope.known.extend(self.context.known.iter().cloned());
                pending.push((body, names.clone(), scope.known));
            }
            self.deferred_expression_list(vec![condition], pending);
            if truth == Some(true) {
                break;
            }
            let mut rest = Scope::default();
            self.refine(condition, &mut rest, false);
            rest.known.extend(self.context.known.iter().cloned());
            if index == last {
                if let Some(body) = otherwise {
                    pending.push((body, names.clone(), rest.known));
                }
            } else {
                self.context.known = rest.known;
            }
        }
        self.context.known = outer_known;
    }

    fn deferred_expressions<'a>(
        &mut self,
        statement: &'a Statement,
        pending: &mut Vec<DeclarationBlock<'a>>,
    ) {
        self.deferred_expression_list(crate::walk::statement_expressions(statement), pending);
    }

    fn deferred_expression_list<'a>(
        &mut self,
        mut expressions: Vec<&'a Expression>,
        pending: &mut Vec<DeclarationBlock<'a>>,
    ) {
        while let Some(expression) = expressions.pop() {
            match &expression.kind {
                ExpressionKind::Closure(closure) => pending.push((
                    &closure.body,
                    self.context.names.clone(),
                    self.context.known.clone(),
                )),
                ExpressionKind::ArrowFunction(arrow) => expressions.push(&arrow.body),
                ExpressionKind::New {
                    class: NewTarget::Anonymous(class),
                    ..
                } => {
                    self.class_declaration(class);
                    self.deferred_members(class, pending);
                }
                _ => {}
            }
            expressions.extend(crate::walk::children(expression));
        }
    }
}
