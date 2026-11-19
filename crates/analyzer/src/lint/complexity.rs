//! Count decisions in `function f($x) { if ($x) { return 1; } return 0; }`.
//! Each callable starts at cyclomatic complexity one and cognitive complexity zero.

use super::{
    Context,
    rule::{Check, DefaultStatus, Group, Rule, Visit},
    traversal::{Node, Scope},
};
use phera_ast::{
    BinaryOperator, ExpressionKind as Expr, Parameter, StatementKind as Stmt, TypeKind,
};
use phera_source::Span;
use std::collections::HashMap;

pub(super) const RULES: &[Rule] = &[
    rule(
        "cyclomatic-complexity",
        "Limit the number of decisions per callable.",
        "function f() { if (a()) {} if (b()) {} }",
        "function f() { if (a()) {} }",
        Visit::Namespace,
        None,
    ),
    rule(
        "cognitive-complexity",
        "Limit nested decisions and changes in control flow.",
        "function f() { if (a()) { if (b()) {} } }",
        "function f() { if (a()) {} }",
        Visit::Namespace,
        None,
    ),
    rule(
        "nesting-depth",
        "Limit control-flow nesting within a callable.",
        "function f() { if (a()) { if (b()) {} } }",
        "function f() { if (a()) {} }",
        Visit::Namespace,
        None,
    ),
    rule(
        "function-length",
        "Limit callable length in source lines.",
        "function f() {\n    first();\n    second();\n}",
        "function f() { first(); }",
        Visit::Namespace,
        None,
    ),
    rule(
        "class-length",
        "Limit class-like declaration length in source lines.",
        "class C {\n    public $first;\n    public $second;\n}",
        "class C {}",
        Visit::Class,
        Some(class_length),
    ),
    rule(
        "parameter-count",
        "Limit the number of declared parameters per callable.",
        "function f($a, $b, $c) {}",
        "function f($a, $b) {}",
        Visit::Namespace,
        None,
    ),
    rule(
        "boolean-parameter",
        "Review boolean parameters that may toggle callable behavior.",
        "function find(bool $includeDrafts) {}",
        "function findIncludingDrafts() {}",
        Visit::Namespace,
        None,
    ),
];

const fn rule(
    name: &'static str,
    description: &'static str,
    bad: &'static str,
    good: &'static str,
    visit: Visit,
    check: Option<Check>,
) -> Rule {
    Rule {
        name,
        description,
        bad,
        good,
        visit,
        check,
        default: DefaultStatus::OptIn,
        fix: "",
        group: Group::Complexity,
    }
}

#[derive(Default)]
pub(super) struct Counts {
    callables: HashMap<usize, Metrics>,
}

struct Metrics {
    span: Span,
    cyclomatic: usize,
    cognitive: usize,
    nesting: usize,
}

impl Counts {
    pub(super) fn observe(&mut self, context: &mut Context<'_>, node: Node<'_>, scope: Scope<'_>) {
        let declaration = match node {
            Node::Method(method) => Some((method.span, method.parameters.as_slice())),
            Node::Hook(hook) => Some((hook.span, hook.parameters.as_slice())),
            Node::Statement(statement) => match &statement.kind {
                Stmt::Function(function) => Some((function.span, function.parameters.as_slice())),
                _ => None,
            },
            Node::Expression(expression, _) => match &expression.kind {
                Expr::Closure(closure) => Some((expression.span, closure.parameters.as_slice())),
                Expr::ArrowFunction(arrow) => Some((expression.span, arrow.parameters.as_slice())),
                _ => None,
            },
            _ => None,
        };
        if let Some((span, parameters)) = declaration {
            self.callables.insert(
                span.start,
                Metrics {
                    span,
                    cyclomatic: 1,
                    cognitive: 0,
                    nesting: 0,
                },
            );
            callable_limits(context, span, parameters);
            return;
        }
        let Some(count) = scope
            .callable
            .and_then(|callable| self.callables.get_mut(&callable))
        else {
            return;
        };
        count.nesting = count.nesting.max(scope.nesting);
        let (decisions, cognitive) = decisions(node);
        count.cyclomatic += decisions;
        if cognitive != 0 {
            count.cognitive += cognitive + scope.nesting;
            count.nesting = count.nesting.max(scope.nesting + 1);
        }
    }

    pub(super) fn report(self, context: &mut Context<'_>) {
        for metric in self.callables.into_values() {
            let thresholds = &context.options.thresholds;
            for (rule, label, value, maximum) in [
                (
                    "cyclomatic-complexity",
                    "Cyclomatic complexity",
                    metric.cyclomatic,
                    thresholds.cyclomatic,
                ),
                (
                    "cognitive-complexity",
                    "Cognitive complexity",
                    metric.cognitive,
                    thresholds.cognitive,
                ),
                (
                    "nesting-depth",
                    "Control-flow nesting",
                    metric.nesting,
                    thresholds.nesting,
                ),
            ] {
                limit(context, rule, metric.span, label, value, maximum);
            }
        }
    }
}

fn decisions(node: Node<'_>) -> (usize, usize) {
    match node {
        Node::Statement(statement) => match &statement.kind {
            Stmt::If { .. }
            | Stmt::For { .. }
            | Stmt::Foreach { .. }
            | Stmt::While { .. }
            | Stmt::DoWhile { .. } => (1, 1),
            Stmt::Switch { cases, .. } => (
                cases.iter().filter(|case| case.condition.is_some()).count(),
                1,
            ),
            Stmt::Try { catches, .. } => (catches.len(), catches.len()),
            _ => (0, 0),
        },
        Node::Expression(expression, _) => match &expression.kind {
            Expr::Ternary { .. } => (1, 1),
            Expr::Match(expression) => (
                expression
                    .arms
                    .iter()
                    .filter(|arm| arm.conditions.is_some())
                    .count(),
                1,
            ),
            Expr::Binary {
                operator: BinaryOperator::And | BinaryOperator::Or | BinaryOperator::NullCoalesce,
                ..
            } => (1, 1),
            _ => (0, 0),
        },
        _ => (0, 0),
    }
}

fn callable_limits(context: &mut Context<'_>, span: Span, parameters: &[Parameter]) {
    let thresholds = &context.options.thresholds;
    let length = context
        .lines
        .line(span.end.saturating_sub(1))
        .saturating_sub(context.lines.line(span.start))
        + 1;
    limit(
        context,
        "function-length",
        span,
        "Callable length in lines",
        length,
        thresholds.function_lines,
    );
    limit(
        context,
        "parameter-count",
        span,
        "Parameter count",
        parameters.len(),
        thresholds.parameters,
    );
    for parameter in parameters {
        if parameter.ty.as_ref().is_some_and(|ty| {
            matches!(&ty.kind, TypeKind::Named(name) if name.text.eq_ignore_ascii_case("bool"))
        }) {
            context.report("boolean-parameter", parameter.span, format!("Review the boolean parameter ${}.", parameter.name));
        }
    }
}

fn class_length(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Class(class) = node else {
        return;
    };
    let length = context
        .lines
        .line(class.span.end.saturating_sub(1))
        .saturating_sub(context.lines.line(class.span.start))
        + 1;
    limit(
        context,
        "class-length",
        class.span,
        "Class length in lines",
        length,
        context.options.thresholds.class_lines,
    );
}

fn limit(
    context: &mut Context<'_>,
    rule: &'static str,
    span: Span,
    label: &str,
    value: usize,
    maximum: usize,
) {
    if value > maximum {
        context.report(
            rule,
            span,
            format!("{label} is {value}, above the configured limit of {maximum}."),
        );
    }
}
