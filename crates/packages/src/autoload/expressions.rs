//! Walk expression children for class declarations hidden in closures or anonymous methods.
//!
//! `$factory = function () { class Deferred {} };` still contributes Deferred to the map.

use super::discovery::{declaration, statements};
use phera_ast::{
    Arguments, Expression, ExpressionKind, MemberName, NewTarget, StringPart, Variable,
};

pub(super) fn expression<'a>(root: &'a Expression, namespace: &str, names: &mut Vec<String>) {
    let mut pending: Vec<&'a Expression> = vec![root];
    while let Some(value) = pending.pop() {
        match &value.kind {
            ExpressionKind::Closure(closure) => statements(&closure.body, namespace, names),
            ExpressionKind::ArrowFunction(arrow) => pending.push(&arrow.body),
            ExpressionKind::New { class, arguments } => {
                match class {
                    NewTarget::Class(value) => pending.push(value),
                    NewTarget::Anonymous(class) => declaration(class, namespace, names),
                }
                arguments_to_visit(arguments, &mut pending);
            }
            ExpressionKind::Binary { left, right, .. } => pending.extend([&**left, &**right]),
            ExpressionKind::Assign { target, value }
            | ExpressionKind::AssignReference { target, value }
            | ExpressionKind::CompoundAssign { target, value, .. } => {
                pending.extend([&**target, &**value]);
            }
            ExpressionKind::Unary { operand, .. }
            | ExpressionKind::Cast { operand, .. }
            | ExpressionKind::Empty(operand)
            | ExpressionKind::Eval(operand)
            | ExpressionKind::Print(operand)
            | ExpressionKind::Throw(operand)
            | ExpressionKind::YieldFrom(operand) => pending.push(operand),
            ExpressionKind::Index { target, index } => {
                pending.push(target);
                if let Some(index) = index {
                    pending.push(index);
                }
            }
            ExpressionKind::Property { target, name, .. }
            | ExpressionKind::ClassConstant {
                class: target,
                name,
            } => {
                pending.push(target);
                member_to_visit(name, &mut pending);
            }
            ExpressionKind::MethodCall {
                target,
                name,
                arguments,
                ..
            }
            | ExpressionKind::StaticCall {
                class: target,
                name,
                arguments,
            } => {
                pending.push(target);
                member_to_visit(name, &mut pending);
                arguments_to_visit(arguments, &mut pending);
            }
            ExpressionKind::StaticProperty { class, name } => {
                pending.push(class);
                if let Variable::Dynamic(value) = name {
                    pending.push(value);
                }
            }
            ExpressionKind::Call { callee, arguments } => {
                pending.push(callee);
                arguments_to_visit(arguments, &mut pending);
            }
            ExpressionKind::IncDec { target, .. } => pending.push(target),
            ExpressionKind::Ternary {
                condition,
                then,
                otherwise,
            } => {
                pending.extend([&**condition, &**otherwise]);
                if let Some(then) = then {
                    pending.push(then);
                }
            }
            ExpressionKind::InstanceOf { value, class } => pending.extend([&**value, &**class]),
            ExpressionKind::Isset(values) => pending.extend(values),
            ExpressionKind::Include { path, .. } => pending.push(path),
            ExpressionKind::Exit(Some(value)) => pending.push(value),
            ExpressionKind::Clone { object, with } => {
                pending.push(object);
                if let Some(value) = with {
                    pending.push(value);
                }
            }
            ExpressionKind::Yield { key, value } => {
                if let Some(value) = key {
                    pending.push(value);
                }
                if let Some(value) = value {
                    pending.push(value);
                }
            }
            ExpressionKind::Array(items) => {
                for item in items {
                    pending.push(&item.value);
                    if let Some(key) = &item.key {
                        pending.push(key);
                    }
                }
            }
            ExpressionKind::List(items) => {
                for item in items.iter().flatten() {
                    pending.push(&item.value);
                    if let Some(key) = &item.key {
                        pending.push(key);
                    }
                }
            }
            ExpressionKind::InterpolatedString(parts) => {
                for part in parts {
                    if let StringPart::Expression(value) = part {
                        pending.push(value);
                    }
                }
            }
            ExpressionKind::Variable(Variable::Dynamic(value)) => pending.push(value),
            ExpressionKind::Match(value) => {
                pending.push(&value.subject);
                for arm in &value.arms {
                    pending.push(&arm.body);
                    if let Some(conditions) = &arm.conditions {
                        pending.extend(conditions);
                    }
                }
            }
            _ => {}
        }
    }
}

fn arguments_to_visit<'a>(arguments: &'a Arguments, pending: &mut Vec<&'a Expression>) {
    if let Arguments::List(arguments) = arguments {
        pending.extend(arguments.iter().map(|argument| &argument.value));
    }
}

fn member_to_visit<'a>(member: &'a MemberName, pending: &mut Vec<&'a Expression>) {
    if let MemberName::Expression(value) = member {
        pending.push(value);
    }
}
