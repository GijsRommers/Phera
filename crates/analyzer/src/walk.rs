//! Structural traversal without recursion: `$a->b()->c()->d()` may be very long.
//! Scope-sensitive execution lives elsewhere; these helpers only enumerate nodes.

use phera_ast::*;

pub fn statement_expressions(statement: &Statement) -> Vec<&Expression> {
    match &statement.kind {
        StatementKind::Expression(e) => vec![e],
        StatementKind::Echo(items) | StatementKind::Unset(items) => items.iter().collect(),
        StatementKind::Return(e) => e.iter().collect(),
        StatementKind::If {
            condition, elseifs, ..
        } => std::iter::once(condition)
            .chain(elseifs.iter().map(|elseif| &elseif.condition))
            .collect(),
        StatementKind::While { condition, .. } | StatementKind::DoWhile { condition, .. } => {
            vec![condition]
        }
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
    match &expression.kind {
        ExpressionKind::Array(_)
        | ExpressionKind::List(_)
        | ExpressionKind::InterpolatedString(_) => aggregate_children(expression),
        ExpressionKind::Call { .. }
        | ExpressionKind::MethodCall { .. }
        | ExpressionKind::StaticCall { .. }
        | ExpressionKind::New { .. } => call_children(expression),
        ExpressionKind::Ternary { .. }
        | ExpressionKind::Isset(_)
        | ExpressionKind::Exit(_)
        | ExpressionKind::Clone { .. }
        | ExpressionKind::Yield { .. }
        | ExpressionKind::Match(_) => branch_children(expression),
        _ => simple_children(expression),
    }
}

fn aggregate_children(expression: &Expression) -> Vec<&Expression> {
    let mut result: Vec<&Expression> = Vec::new();
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
                if let StringPart::Expression(e) | StringPart::DollarCurly(e, _) = part {
                    result.push(e);
                }
            }
        }
        _ => {}
    }
    result
}

fn call_children(expression: &Expression) -> Vec<&Expression> {
    let mut result: Vec<&Expression> = Vec::new();
    match &expression.kind {
        ExpressionKind::Call {
            callee, arguments, ..
        } => {
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
        _ => {}
    }
    result
}

fn branch_children(expression: &Expression) -> Vec<&Expression> {
    let mut result: Vec<&Expression> = Vec::new();
    match &expression.kind {
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

fn simple_children(expression: &Expression) -> Vec<&Expression> {
    let mut result: Vec<&Expression> = Vec::new();
    match &expression.kind {
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
        ExpressionKind::StaticProperty { class, name, .. } => {
            result.push(class);
            if let Variable::Dynamic(e) = name {
                result.push(e);
            }
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
