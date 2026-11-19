//! In `foreach ($items as &$item) {} $item = 0;`, the second write can overwrite
//! the array's last element. `unset($item)` releases the reference.

use super::{
    Context,
    rule::Group,
    suspicious::variable,
    traversal::{Node, Scope},
};
use phera_ast::{ExpressionKind as Expr, StatementKind as Stmt, Variable};
use phera_source::Span;
use std::collections::HashMap;

pub(super) const NAME: &str = "foreach-reference-not-unset";

type Binding<'a> = (Option<usize>, &'a str);

#[derive(Default)]
pub(super) struct References<'a> {
    loops: Vec<Loop<'a>>,
    events: HashMap<Binding<'a>, Vec<(usize, Action)>>,
    continuations: Vec<Continuation>,
    by_block_start: HashMap<usize, usize>,
}

struct Loop<'a> {
    binding: Binding<'a>,
    span: Span,
    after: usize,
    limit: usize,
    continuation: Option<usize>,
}

struct Continuation {
    after: usize,
    limit: usize,
    next: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Action {
    Release,
    Use,
}

impl<'a> References<'a> {
    pub(super) fn observe(&mut self, node: Node<'a>, scope: Scope<'a>) {
        let binding = |name| (scope.callable, name);
        match node {
            Node::Block(body) => {
                let mut continuation = body
                    .first()
                    .and_then(|first| self.by_block_start.get(&first.span.start).copied());
                let mut limit = body.last().map_or(0, |last| last.span.end);
                for statement in body.iter().rev() {
                    match &statement.kind {
                        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return(_) | Stmt::Goto(_) => {
                            limit = statement.span.start;
                            continuation = None;
                        }
                        Stmt::Expression(expr)
                            if matches!(expr.kind, Expr::Throw(_) | Expr::Exit(_)) =>
                        {
                            limit = statement.span.start;
                            continuation = None;
                        }
                        Stmt::Foreach {
                            value,
                            by_reference: true,
                            ..
                        } => {
                            if let Some(name) = variable(value) {
                                self.events.entry(binding(name)).or_default();
                                self.loops.push(Loop {
                                    binding: binding(name),
                                    span: value.span,
                                    after: statement.span.end,
                                    limit,
                                    continuation,
                                });
                            }
                        }
                        Stmt::Block(body) => {
                            self.remember_continuation(
                                body,
                                statement.span.end,
                                limit,
                                continuation,
                            );
                        }
                        Stmt::If {
                            then,
                            elseifs,
                            otherwise,
                            ..
                        } => {
                            self.remember_continuation(
                                then,
                                statement.span.end,
                                limit,
                                continuation,
                            );
                            for branch in elseifs {
                                self.remember_continuation(
                                    &branch.body,
                                    statement.span.end,
                                    limit,
                                    continuation,
                                );
                            }
                            if let Some(otherwise) = otherwise {
                                self.remember_continuation(
                                    otherwise,
                                    statement.span.end,
                                    limit,
                                    continuation,
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
            Node::Statement(statement) => match &statement.kind {
                Stmt::Unset(values) => {
                    for value in values {
                        if let Some(name) = variable(value) {
                            self.event(binding(name), value.span.start, Action::Release);
                        }
                    }
                }
                Stmt::Foreach {
                    value,
                    by_reference: true,
                    ..
                } => {
                    if let Some(name) = variable(value) {
                        self.event(binding(name), value.span.start, Action::Release);
                    }
                }
                Stmt::Global(variables) => {
                    for variable in variables {
                        if let Variable::Named(name) = variable {
                            self.event(binding(name), statement.span.start, Action::Release);
                        }
                    }
                }
                _ => {}
            },
            Node::Expression(expression, _) => {
                if let Some(name) = variable(expression) {
                    self.event(binding(name), expression.span.start, Action::Use);
                }
                if let Expr::AssignReference { target, .. } = &expression.kind
                    && let Some(name) = variable(target)
                {
                    self.event(binding(name), target.span.start, Action::Release);
                }
            }
            _ => {}
        }
    }

    fn remember_continuation(
        &mut self,
        body: &[phera_ast::Statement],
        after: usize,
        limit: usize,
        next: Option<usize>,
    ) {
        if let Some(first) = body.first() {
            let index = self.continuations.len();
            self.continuations.push(Continuation { after, limit, next });
            self.by_block_start.insert(first.span.start, index);
        }
    }

    fn event(&mut self, binding: Binding<'a>, offset: usize, action: Action) {
        if let Some(events) = self.events.get_mut(&binding) {
            events.push((offset, action));
        }
    }

    pub(super) fn report(mut self, context: &mut Context<'_>) {
        let explicit = context.options.explicitly_enabled(NAME, Group::Suspicious);
        for events in self.events.values_mut() {
            events.sort_unstable();
        }
        for reference in self.loops {
            let Some(events) = self.events.get(&reference.binding) else {
                continue;
            };
            let first_action = |after, limit| {
                let first = events.partition_point(|&(offset, _)| offset < after);
                events.get(first).filter(|&&(offset, _)| offset < limit)
            };
            let mut action = first_action(reference.after, reference.limit);
            let mut next = reference.continuation;
            while action.is_none() {
                let Some(segment) = next.and_then(|index| self.continuations.get(index)) else {
                    break;
                };
                action = first_action(segment.after, segment.limit);
                next = segment.next;
            }
            if matches!(action, Some((_, Action::Use))) || (action.is_none() && explicit) {
                let name = reference.binding.1;
                context.report(
                    NAME,
                    reference.span,
                    format!("Unset ${name} after foreach to release the last element reference."),
                );
            }
        }
    }
}
