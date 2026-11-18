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
                otherwise,
            } => {
                let scope = Scope::default();
                let truth = self.condition_truth(condition, &scope);
                for (branch, value) in [(Some(&then[..]), true), (otherwise.as_deref(), false)] {
                    if let Some(branch) = branch
                        && truth.is_none_or(|truth| truth == value)
                    {
                        let mut scope = scope.clone();
                        self.refine(condition, &mut scope, value);
                        scope.known.extend(known.iter().cloned());
                        pending.push((branch, names.clone(), scope.known));
                    }
                }
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

    fn deferred_expressions<'a>(
        &mut self,
        statement: &'a Statement,
        pending: &mut Vec<DeclarationBlock<'a>>,
    ) {
        let mut expressions = crate::walk::statement_expressions(statement);
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
