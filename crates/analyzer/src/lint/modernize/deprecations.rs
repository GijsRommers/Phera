//! PHP 8.5 deprecates constructs such as `(integer) $n` and `strftime('%Y')`.

use super::rule;
use crate::lint::{
    Context,
    rule::{Rule, Visit},
    traversal::Node,
};
use phera_ast::{
    DollarCurlyForm, ExpressionKind as Expr, MemberName, Name, NewTarget, StringPart, UseKind,
};
use phera_source::{
    Span,
    deprecations::{self, ItemKind},
};

pub(super) const RULES: &[Rule] = &[
    rule(
        "deprecated-php-item",
        "Report statically resolved PHP names deprecated in PHP 8.5.",
        "$time = strftime('%Y');",
        "$time = date('Y');",
        "Replacement notes come from PHP's stub inventory.",
        Visit::Expression,
        deprecated_item,
    ),
    rule(
        "deprecated-syntax",
        "Detect deprecated casts and dollar-curly interpolation.",
        "$n = (integer) $value;",
        "$n = (int) $value;",
        "",
        Visit::Expression,
        deprecated_syntax,
    ),
];

fn deprecated_item(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expr, use_kind) = node else {
        return;
    };
    let (kind, name) = match &expr.kind {
        Expr::Name(name) if use_kind == UseKind::Constant => {
            let Some(name) = resolve(context, name, use_kind) else {
                return;
            };
            (ItemKind::Constant, name)
        }
        Expr::Call { callee, .. } => {
            let Expr::Name(name) = &callee.kind else {
                return;
            };
            let Some(name) = resolve(context, name, UseKind::Function) else {
                return;
            };
            (ItemKind::Function, name)
        }
        Expr::ClassConstant {
            class,
            name: MemberName::Identifier(member),
        }
        | Expr::StaticCall {
            class,
            name: MemberName::Identifier(member),
            ..
        } => {
            let Expr::Name(class) = &class.kind else {
                return;
            };
            let Some(class) = resolve(context, class, UseKind::Class) else {
                return;
            };
            let kind = if matches!(expr.kind, Expr::StaticCall { .. }) {
                ItemKind::Method
            } else {
                ItemKind::ClassConstant
            };
            (kind, format!("{class}::{}", member.name))
        }
        Expr::MethodCall {
            target,
            name: MemberName::Identifier(member),
            ..
        } => {
            let Expr::New {
                class: NewTarget::Class(class),
                ..
            } = &target.kind
            else {
                return;
            };
            let Expr::Name(class) = &class.kind else {
                return;
            };
            let Some(class) = resolve(context, class, UseKind::Class) else {
                return;
            };
            (ItemKind::Method, format!("{class}::{}", member.name))
        }
        _ => return,
    };
    if let Some(item) = deprecations::find(kind, &name).filter(|item| !item.since.is_empty()) {
        report_item(context, expr.span, item.name, item.since, item.message);
    }
}

fn report_item(context: &mut Context<'_>, span: Span, name: &str, since: &str, replacement: &str) {
    let message = if replacement.is_empty() {
        format!("{name} is deprecated since PHP {since}.")
    } else {
        format!(
            "{name} is deprecated since PHP {since}, {}.",
            replacement.trim_end_matches('.')
        )
    };
    context.report("deprecated-php-item", span, message);
}

fn resolve(context: &Context<'_>, name: &Name, kind: UseKind) -> Option<String> {
    let resolved = context.names.resolve(name, kind);
    match resolved.as_slice() {
        [name] => Some(name.clone()),
        _ => None,
    }
}

fn deprecated_syntax(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expr, _) = node else {
        return;
    };
    match &expr.kind {
        Expr::Cast { operand, .. } => {
            let prefix = context
                .source
                .slice(Span::new(expr.span.start, operand.span.start));
            let old = prefix
                .iter()
                .copied()
                .filter(|b| !b.is_ascii_whitespace() && !matches!(b, b'(' | b')'))
                .map(|b| b.to_ascii_lowercase())
                .collect::<Vec<_>>();
            let replacement = match old.as_slice() {
                b"integer" => "int",
                b"boolean" => "bool",
                b"double" => "float",
                b"binary" => "string",
                _ => return,
            };
            context.report(
                "deprecated-syntax",
                expr.span,
                format!("This cast spelling is deprecated in PHP 8.5. Use ({replacement})."),
            );
        }
        Expr::InterpolatedString(parts) => {
            for part in parts {
                if let StringPart::DollarCurly(value, form) = part {
                    let replacement = match form {
                        DollarCurlyForm::Name => "Move the dollar sign inside the braces.",
                        DollarCurlyForm::Expression => "Use explicit variable-variable braces.",
                    };
                    context.report(
                        "deprecated-syntax",
                        value.span,
                        format!(
                            "Dollar-curly interpolation is deprecated since PHP 8.2. {replacement}"
                        ),
                    );
                }
            }
        }
        _ => {}
    }
}
