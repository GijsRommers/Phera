//! In `function f() { $value = 1; }`, the local binding is never read.
//! PHP's dynamic scope operations can observe every local, so they opt a
//! callable out of unused-binding checks.

use super::rule::{DefaultStatus, Group, Rule, Visit};
use super::{
    Context,
    traversal::{Node, Scope},
};
use phera_ast::{
    Arguments, Expression, ExpressionKind as Expr, Literal, NameKind, Parameter,
    StatementKind as Stmt, Variable, Visibility,
};
use phera_source::Span;
use std::collections::{HashMap, HashSet};

pub(super) const RULES: &[Rule] = &[
    Rule {
        default: DefaultStatus::Enabled,
        ..local(
            "unused-variable",
            "Detect standalone assignments to locals whose values are never read.",
            "function f() { $a = 1; }",
            "function f() {}",
        )
    },
    local(
        "unused-parameter",
        "Detect unused parameters of closures and private methods.",
        "$f = function ($a) { return 1; };",
        "$f = function () { return 1; };",
    ),
    local(
        "unused-closure-use",
        "Detect a closure capture that is never read.",
        "$f = function () use ($a) {};",
        "$f = function () {};",
    ),
    local(
        "unused-catch-variable",
        "Suggest a catch without a binding when the variable is never read.",
        "try { f(); } catch (\\Exception $e) { recover(); }",
        "try { f(); } catch (\\Exception) { recover(); }",
    ),
    local(
        "unused-label",
        "Detect labels that are never targeted by goto.",
        "ready: run();",
        "run();",
    ),
    Rule {
        group: Group::Suspicious,
        ..local(
            "compact-undefined-variable",
            "Detect literal compact() names with no syntactic binding.",
            "function f() { return compact('missing'); }",
            "function f() { $value = 1; return compact('value'); }",
        )
    },
];

const fn local(
    name: &'static str,
    description: &'static str,
    bad: &'static str,
    good: &'static str,
) -> Rule {
    Rule {
        name,
        description,
        bad,
        good,
        group: Group::Unused,
        default: DefaultStatus::OptIn,
        fix: "",
        visit: Visit::Namespace,
        check: None,
    }
}

#[derive(Default)]
struct Bindings<'a> {
    writes: HashMap<&'a str, Span>,
    write_targets: HashSet<usize>,
    standalone_assignments: HashSet<usize>,
    unused_candidates: HashMap<&'a str, Span>,
    reads: HashSet<&'a str>,
    parameters: HashSet<&'a str>,
    candidates: Vec<Candidate<'a>>,
    labels: Vec<(&'a str, Span)>,
    jumps: HashSet<&'a str>,
    compact: Vec<(&'a str, Span)>,
    arrow_parent: Option<usize>,
    dynamic: bool,
}

struct Candidate<'a> {
    rule: &'static str,
    name: &'a str,
    span: Span,
}

#[derive(Default)]
pub(super) struct Locals<'a> {
    scopes: HashMap<Option<usize>, Bindings<'a>>,
}

impl<'a> Locals<'a> {
    pub(super) fn observe(&mut self, node: Node<'a>, scope: Scope<'a>) {
        match node {
            Node::Method(method) if method.body.is_some() => {
                let bindings = self.scopes.entry(Some(method.span.start)).or_default();
                let private = method.modifiers.visibility == Some(Visibility::Private)
                    && method.attributes.is_empty()
                    && !method.name.name.starts_with("__");
                bindings.parameters(&method.parameters, private);
            }
            Node::Hook(hook) => {
                let bindings = self.scopes.entry(Some(hook.span.start)).or_default();
                bindings.parameters(&hook.parameters, false);
                if hook.name.name == "set" && !hook.has_parameter_list {
                    bindings.parameters.insert("value");
                }
            }
            Node::Statement(statement) => {
                let bindings = self.scopes.entry(scope.callable).or_default();
                match &statement.kind {
                    Stmt::Function(function) => self
                        .scopes
                        .entry(Some(function.span.start))
                        .or_default()
                        .parameters(&function.parameters, false),
                    Stmt::Expression(expr) if matches!(expr.kind, Expr::Assign { .. }) => {
                        bindings.standalone_assignments.insert(expr.span.start);
                    }
                    Stmt::Try { catches, .. } => {
                        for catch in catches {
                            if let Some(name) = &catch.variable {
                                bindings.candidates.push(Candidate {
                                    rule: "unused-catch-variable",
                                    name,
                                    span: catch.span,
                                });
                            }
                        }
                    }
                    Stmt::Foreach {
                        key,
                        value,
                        by_reference,
                        ..
                    } => {
                        if !by_reference {
                            bindings.write(value, WriteUse::Binding);
                            if let Some(key) = key {
                                bindings.write(key, WriteUse::Binding);
                            }
                        } else if let Some(name) = super::suspicious::variable(value) {
                            // Subsequent writes may still update the last element.
                            bindings.parameters.insert(name);
                        }
                    }
                    Stmt::Goto(name) => {
                        bindings.jumps.insert(&name.name);
                    }
                    Stmt::Label(name) => bindings.labels.push((&name.name, name.span)),
                    Stmt::Global(_) => bindings.dynamic = true,
                    Stmt::Static(variables) => bindings
                        .parameters
                        .extend(variables.iter().map(|v| v.name.as_str())),
                    _ => {}
                }
            }
            Node::Expression(expr, _) => self.expression(expr, scope.callable),
            _ => {}
        }
    }

    fn expression(&mut self, expr: &'a Expression, current: Option<usize>) {
        let bindings = self.scopes.entry(current).or_default();
        match &expr.kind {
            Expr::Variable(Variable::Named(name)) => {
                if !bindings.write_targets.contains(&expr.span.start) {
                    bindings.reads.insert(name);
                }
            }
            Expr::Variable(Variable::Dynamic(_))
            | Expr::Eval(_)
            | Expr::Include { .. }
            | Expr::AssignReference { .. } => bindings.dynamic = true,
            Expr::Assign { target, .. } => {
                let usage = if bindings.standalone_assignments.contains(&expr.span.start) {
                    WriteUse::Standalone
                } else {
                    WriteUse::Binding
                };
                bindings.write(target, usage);
            }
            Expr::Closure(closure) => {
                bindings
                    .reads
                    .extend(closure.uses.iter().map(|capture| capture.name.as_str()));
                let bindings = self.scopes.entry(Some(expr.span.start)).or_default();
                bindings.parameters(&closure.parameters, closure.attributes.is_empty());
                for capture in &closure.uses {
                    bindings.parameters.insert(&capture.name);
                    if !capture.by_reference {
                        bindings.candidates.push(Candidate {
                            rule: "unused-closure-use",
                            name: &capture.name,
                            span: capture.span,
                        });
                    }
                }
            }
            Expr::ArrowFunction(arrow) => {
                let bindings = self.scopes.entry(Some(expr.span.start)).or_default();
                bindings.parameters(&arrow.parameters, arrow.attributes.is_empty());
                bindings.arrow_parent = current;
            }
            Expr::Call {
                callee,
                arguments: Arguments::List(args),
                ..
            } => {
                let Expr::Name(name) = &callee.kind else {
                    return;
                };
                if !matches!(name.kind, NameKind::Unqualified | NameKind::FullyQualified) {
                    return;
                }
                match name.text.to_ascii_lowercase().as_str() {
                    "extract" | "get_defined_vars" | "func_get_args" | "func_get_arg"
                    | "func_num_args" => bindings.dynamic = true,
                    "compact" => {
                        for arg in args {
                            if !arg.spread
                                && let Expr::Literal(Literal::String(name)) = &arg.value.kind
                                && let Ok(name) = std::str::from_utf8(name)
                            {
                                bindings.reads.insert(name);
                                bindings.compact.push((name, arg.span));
                            } else {
                                bindings.dynamic = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    pub(super) fn report(mut self, context: &mut Context<'_>) {
        // Nested arrow expressions implicitly read their outer variables. Their
        // source positions order children before parents when traversed backwards.
        let mut arrows: Vec<_> = self
            .scopes
            .iter()
            .filter_map(|(&child, bindings)| Some((child?, bindings.arrow_parent?)))
            .collect();
        arrows.sort_unstable_by(|a, b| b.cmp(a));
        for (child, parent) in arrows {
            if let Some(bindings) = self.scopes.get(&Some(child)) {
                let reads: Vec<_> = bindings
                    .reads
                    .difference(&bindings.parameters)
                    .copied()
                    .collect();
                let parent = self.scopes.entry(Some(parent)).or_default();
                parent.reads.extend(reads);
            }
        }
        for (callable, bindings) in self.scopes {
            for &(name, span) in &bindings.labels {
                if !bindings.jumps.contains(name) {
                    context.report(
                        "unused-label",
                        span,
                        format!("Label '{name}' is never targeted."),
                    );
                }
            }
            if bindings.dynamic || callable.is_none() {
                continue;
            }
            for candidate in &bindings.candidates {
                if !bindings.reads.contains(candidate.name) && !candidate.name.starts_with('_') {
                    let message = match candidate.rule {
                        "unused-catch-variable" => format!(
                            "Catch variable ${} is never read. Omit the variable from catch.",
                            candidate.name
                        ),
                        "unused-closure-use" => {
                            format!("Closure capture ${} is never read.", candidate.name)
                        }
                        _ => format!("Parameter ${} is never read.", candidate.name),
                    };
                    context.report(candidate.rule, candidate.span, message);
                }
            }
            for (name, span) in &bindings.unused_candidates {
                if !bindings.reads.contains(name)
                    && !bindings.is_binding(name, false)
                    && !name.starts_with('_')
                {
                    context.report(
                        "unused-variable",
                        *span,
                        format!("Local variable ${name} is assigned but never read."),
                    );
                }
            }
            for (name, span) in &bindings.compact {
                if !bindings.is_binding(name, true) {
                    context.report(
                        "compact-undefined-variable",
                        *span,
                        format!("compact() refers to unbound local ${name}."),
                    );
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WriteUse {
    Binding,
    Standalone,
}

impl<'a> Bindings<'a> {
    fn parameters(&mut self, parameters: &'a [Parameter], report: bool) {
        for param in parameters {
            self.parameters.insert(&param.name);
            if report && !param.by_reference && !param.is_promoted() {
                self.candidates.push(Candidate {
                    rule: "unused-parameter",
                    name: &param.name,
                    span: param.span,
                });
            }
        }
    }

    fn write(&mut self, root: &'a Expression, usage: WriteUse) {
        let mut pending = vec![root];
        while let Some(target) = pending.pop() {
            match &target.kind {
                Expr::Variable(Variable::Named(name)) => {
                    self.write_targets.insert(target.span.start);
                    remember_first(&mut self.writes, name, target.span);
                    if usage == WriteUse::Standalone {
                        remember_first(&mut self.unused_candidates, name, target.span);
                    }
                }
                Expr::List(items) => pending.extend(items.iter().flatten().map(|item| &item.value)),
                _ => {}
            }
        }
    }

    fn is_binding(&self, name: &str, include_writes: bool) -> bool {
        self.parameters.contains(name)
            || include_writes && self.writes.contains_key(name)
            || self.candidates.iter().any(|c| c.name == name)
            || matches!(
                name,
                "this"
                    | "GLOBALS"
                    | "_SERVER"
                    | "_GET"
                    | "_POST"
                    | "_FILES"
                    | "_COOKIE"
                    | "_SESSION"
                    | "_REQUEST"
                    | "_ENV"
            )
    }
}

fn remember_first<'a>(writes: &mut HashMap<&'a str, Span>, name: &'a str, position: Span) {
    writes
        .entry(name)
        .and_modify(|first| {
            if position.start < first.start {
                *first = position;
            }
        })
        .or_insert(position);
}
