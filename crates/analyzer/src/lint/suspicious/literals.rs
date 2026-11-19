//! PHP treats `'1'` and `1` as the same array key, but match compares them strictly.

use super::{Context, Node};
use phera_ast::{Expression, ExpressionKind, Literal, StatementKind, UnaryOperator};
use std::collections::HashSet;

#[derive(Hash, PartialEq, Eq)]
enum Scalar<'a> {
    Integer(i64),
    Float(u64),
    String(&'a [u8]),
    Boolean(bool),
    Null,
}

fn scalar(expression: &Expression) -> Option<Scalar<'_>> {
    match &expression.kind {
        ExpressionKind::Literal(literal) => match literal {
            Literal::Int(n) => Some(Scalar::Integer(*n)),
            Literal::Float(n) => Some(Scalar::Float(float_key(*n))),
            Literal::String(s) => Some(Scalar::String(s)),
            Literal::Bool(b) => Some(Scalar::Boolean(*b)),
            Literal::Null => Some(Scalar::Null),
        },
        ExpressionKind::Unary {
            operator: UnaryOperator::Minus,
            operand,
        } => match &operand.kind {
            ExpressionKind::Literal(Literal::Int(n)) => n.checked_neg().map(Scalar::Integer),
            ExpressionKind::Literal(Literal::Float(n)) => Some(Scalar::Float(float_key(-n))),
            _ => None,
        },
        _ => None,
    }
}

fn float_key(number: f64) -> u64 {
    if number == 0.0 { 0 } else { number.to_bits() }
}

fn array_key(key: Scalar<'_>) -> Option<Scalar<'_>> {
    match key {
        Scalar::String(bytes) => Some(
            std::str::from_utf8(bytes)
                .ok()
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|value| value.to_string().as_bytes() == bytes)
                .map_or(Scalar::String(bytes), Scalar::Integer),
        ),
        Scalar::Boolean(value) => Some(Scalar::Integer(i64::from(value))),
        Scalar::Null => Some(Scalar::String(b"")),
        Scalar::Float(bits) => {
            let float = f64::from_bits(bits);
            // Integral floats convert without losing precision or raising a warning.
            (float.fract() == 0.0 && float >= i64::MIN as f64 && float < -(i64::MIN as f64))
                .then_some(Scalar::Integer(float as i64))
        }
        key => Some(key),
    }
}

fn display(key: &Scalar<'_>) -> String {
    match key {
        Scalar::Integer(n) => n.to_string(),
        Scalar::Float(n) => f64::from_bits(*n).to_string(),
        Scalar::String(s) => format!("{:?}", String::from_utf8_lossy(s)),
        Scalar::Boolean(b) => b.to_string(),
        Scalar::Null => "null".into(),
    }
}

pub(super) fn duplicate_keys(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expr, _) = node else {
        return;
    };
    let ExpressionKind::Array(items) = &expr.kind else {
        return;
    };
    if items.len() < 2 {
        return;
    }
    let mut seen = HashSet::new();
    let mut next_index = Some(0i64);
    let mut initialized = false;
    for item in items {
        if item.spread {
            next_index = None;
            continue;
        }
        if let Some(key) = &item.key {
            let Some(value) = scalar(key).and_then(array_key) else {
                next_index = None;
                continue;
            };
            if let Scalar::Integer(n) = value {
                if next_index.is_some_and(|index| n >= index || !initialized) {
                    next_index = n.checked_add(1);
                }
                initialized = true;
            }
            if seen.contains(&value) {
                context.report(
                    "duplicate-array-key",
                    key.span,
                    format!("Array key {} is repeated.", display(&value)),
                );
            }
            seen.insert(value);
        } else if let Some(n) = next_index {
            initialized = true;
            next_index = n.checked_add(1);
            seen.insert(Scalar::Integer(n));
        }
    }
}

pub(super) fn duplicate_match(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expr, _) = node else {
        return;
    };
    let ExpressionKind::Match(m) = &expr.kind else {
        return;
    };
    duplicates(
        context,
        "duplicate-match-arm",
        "Match condition",
        m.arms
            .iter()
            .filter_map(|arm| arm.conditions.as_ref())
            .flatten(),
    );
}

pub(super) fn duplicate_cases(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Statement(statement) = node else {
        return;
    };
    let StatementKind::Switch { cases, .. } = &statement.kind else {
        return;
    };
    duplicates(
        context,
        "duplicate-switch-case",
        "Switch case",
        cases.iter().filter_map(|case| case.condition.as_ref()),
    );
}

fn duplicates<'a>(
    context: &mut Context<'_>,
    rule: &'static str,
    label: &str,
    expressions: impl Iterator<Item = &'a Expression>,
) {
    let mut seen = HashSet::new();
    for expression in expressions {
        let Some(value) = scalar(expression) else {
            continue;
        };
        if seen.contains(&value) {
            context.report(
                rule,
                expression.span,
                format!("{label} {} is repeated.", display(&value)),
            );
        }
        seen.insert(value);
    }
}
