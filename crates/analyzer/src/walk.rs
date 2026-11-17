//! Structural traversal without recursion: `$a->b()->c()->d()` may be very long.
//! Scope-sensitive execution lives elsewhere; these helpers only enumerate nodes.

use phera_ast::*;

pub fn statement_expressions(statement: &Statement) -> Vec<&Expression> {
    match &statement.kind {
        StatementKind::Expression(e) => vec![e],
        StatementKind::Echo(items) | StatementKind::Unset(items) => items.iter().collect(),
        StatementKind::Return(e) => e.iter().collect(),
        StatementKind::If { condition, .. }
        | StatementKind::While { condition, .. }
        | StatementKind::DoWhile { condition, .. } => vec![condition],
        StatementKind::For {
            init,
            condition,
            step,
            ..
        } => init.iter().chain(condition).chain(step).collect(),
        StatementKind::Foreach { subject, .. } => vec![subject],
        StatementKind::Switch { subject, .. } => vec![subject],
        StatementKind::Static(items) => items.iter().filter_map(|v| v.default.as_ref()).collect(),
        StatementKind::Const { items, .. } => items.iter().map(|v| &v.value).collect(),
        _ => Vec::new(),
    }
}

pub fn children(expression: &Expression) -> Vec<&Expression> {
    let mut result = Vec::new();
    match &expression.kind {
        ExpressionKind::Array(items) => {
            for item in items {
                array_item(item, &mut result);
            }
        }
        ExpressionKind::List(items) => {
            for item in items.iter().flatten() {
                array_item(item, &mut result);
            }
        }
        ExpressionKind::InterpolatedString(parts) => {
            for part in parts {
                if let StringPart::Expression(e) = part {
                    result.push(e);
                }
            }
        }
        ExpressionKind::Variable(Variable::Dynamic(e)) => result.push(e),
        ExpressionKind::Index { target, index } => {
            result.push(target);
            result.extend(index.as_deref());
        }
        ExpressionKind::Property { target, name, .. }
        | ExpressionKind::ClassConstant {
            class: target,
            name,
        } => {
            result.push(target);
            member(name, &mut result);
        }
        ExpressionKind::StaticProperty { class, name } => {
            result.push(class);
            if let Variable::Dynamic(e) = name {
                result.push(e);
            }
        }
        ExpressionKind::Call { callee, arguments } => {
            result.push(callee);
            arguments_children(arguments, &mut result);
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
            result.push(target);
            member(name, &mut result);
            arguments_children(arguments, &mut result);
        }
        ExpressionKind::New { class, arguments } => {
            if let NewTarget::Class(e) = class {
                result.push(e);
            }
            arguments_children(arguments, &mut result);
        }
        ExpressionKind::Unary { operand, .. }
        | ExpressionKind::Cast { operand, .. }
        | ExpressionKind::Empty(operand)
        | ExpressionKind::Eval(operand)
        | ExpressionKind::Print(operand)
        | ExpressionKind::Throw(operand)
        | ExpressionKind::YieldFrom(operand)
        | ExpressionKind::Include { path: operand, .. }
        | ExpressionKind::IncDec {
            target: operand, ..
        } => result.push(operand),
        ExpressionKind::Binary { left, right, .. }
        | ExpressionKind::Assign {
            target: left,
            value: right,
        }
        | ExpressionKind::AssignReference {
            target: left,
            value: right,
        }
        | ExpressionKind::CompoundAssign {
            target: left,
            value: right,
            ..
        }
        | ExpressionKind::InstanceOf {
            value: left,
            class: right,
        } => {
            result.push(left);
            result.push(right);
        }
        ExpressionKind::Ternary {
            condition,
            then,
            otherwise,
        } => {
            result.push(condition);
            result.extend(then.as_deref());
            result.push(otherwise);
        }
        ExpressionKind::Isset(items) => result.extend(items),
        ExpressionKind::Exit(value) => result.extend(value.as_deref()),
        ExpressionKind::Clone { object, with } => {
            result.push(object);
            result.extend(with.as_deref());
        }
        ExpressionKind::Yield { key, value } => {
            result.extend(key.as_deref());
            result.extend(value.as_deref());
        }
        ExpressionKind::Match(m) => {
            result.push(&m.subject);
            for arm in &m.arms {
                if let Some(conditions) = &arm.conditions {
                    result.extend(conditions);
                }
                result.push(&arm.body);
            }
        }
        _ => {}
    }
    result
}

fn array_item<'a>(item: &'a ArrayItem, result: &mut Vec<&'a Expression>) {
    result.extend(item.key.as_ref());
    result.push(&item.value);
}

fn member<'a>(name: &'a MemberName, result: &mut Vec<&'a Expression>) {
    if let MemberName::Expression(e) = name {
        result.push(e);
    }
}

fn arguments_children<'a>(arguments: &'a Arguments, result: &mut Vec<&'a Expression>) {
    if let Arguments::List(items) = arguments {
        result.extend(items.iter().map(|a| &a.value));
    }
}
