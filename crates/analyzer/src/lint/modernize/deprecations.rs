//! Deprecated PHP constructs include `(integer) $n` and PHP 8.6's `spl_object_hash($o)`.

use super::{callables::direct_function, rule};
use crate::lint::{
    Context, Language,
    rule::{Rule, Visit},
    traversal::Node,
};
use phera_ast::{
    Arguments, ClassMemberKind as Member, DollarCurlyForm, ExpressionKind as Expr, Literal,
    MemberName, Name, NewTarget, StatementKind as Stmt, StringPart, TypeKind, UseKind,
};
use phera_source::{
    Span,
    deprecations::{self, ItemKind},
};

pub(super) const RULES: &[Rule] = &[
    rule(
        "deprecated-php-item",
        "Report statically resolved PHP names deprecated at the selected language level.",
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
    rule(
        "php86-deprecated-name",
        "Detect declarations and import aliases deprecated in PHP 8.6.",
        "function let() {}",
        "function initialize() {}",
        "",
        Visit::Statement,
        deprecated_name,
    ),
    rule(
        "php86-deprecated-call",
        "Detect deprecated argument forms and calls introduced in PHP 8.6.",
        "define('N', 1, false);",
        "define('N', 1);",
        "",
        Visit::Expression,
        deprecated_call,
    ),
    rule(
        "php86-deprecated-magic-method",
        "Detect deprecated constructor, destructor and __debugInfo declarations in PHP 8.6.",
        "class C { function __debugInfo(): ?array { return null; } }",
        "class C { function __debugInfo(): array { return []; } }",
        "",
        Visit::Method,
        deprecated_method,
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
    } else if context.language == Language::Php86
        && let Some(message) = deprecation_86(kind, &name)
    {
        report_item(context, expr.span, &name, "8.6", message);
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

fn deprecation_86(kind: ItemKind, name: &str) -> Option<&'static str> {
    use std::{collections::HashMap, sync::OnceLock};
    type Table = HashMap<(ItemKind, String), &'static str>;
    static ITEMS: OnceLock<Table> = OnceLock::new();
    let key = |kind, name: &str| match kind {
        ItemKind::Function | ItemKind::Method => name.to_ascii_lowercase(),
        ItemKind::Constant => name.to_string(),
        ItemKind::ClassConstant => name.split_once("::").map_or_else(
            || name.to_string(),
            |(class, constant)| format!("{}::{constant}", class.to_ascii_lowercase()),
        ),
    };
    let items = ITEMS.get_or_init(|| {
        include_str!("../../../data/php86-deprecations.tsv")
            .lines()
            .filter_map(|line| {
                let mut fields = line.splitn(3, '\t');
                let kind = match fields.next()? {
                    "function" => ItemKind::Function,
                    "method" => ItemKind::Method,
                    "constant" => ItemKind::Constant,
                    "class-constant" => ItemKind::ClassConstant,
                    _ => return None,
                };
                Some((
                    (kind, key(kind, fields.next().unwrap_or_default())),
                    fields.next().unwrap_or_default(),
                ))
            })
            .collect()
    });
    items.get(&(kind, key(kind, name))).copied()
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

fn deprecated_name(context: &mut Context<'_>, node: Node<'_>) {
    if context.language != Language::Php86 {
        return;
    }
    let Node::Statement(statement) = node else {
        return;
    };
    let reserved = |name: &str| name.eq_ignore_ascii_case("let") || name.eq_ignore_ascii_case("is");
    let mut report = |span, name: &str| {
        context.report(
            "php86-deprecated-name",
            span,
            format!(
                "The name '{name}' is deprecated here in PHP 8.6. Rename the declaration or alias."
            ),
        );
    };
    match &statement.kind {
        Stmt::Function(function)
            if reserved(&function.name.name)
                || function.name.name.eq_ignore_ascii_case("readonly") =>
        {
            report(function.name.span, &function.name.name)
        }
        Stmt::Class(class) => {
            if let Some(name) = &class.name
                && reserved(&name.name)
            {
                report(name.span, &name.name);
            }
            for member in &class.members {
                if let Member::Constant { items, .. } = &member.kind {
                    for item in items
                        .iter()
                        .filter(|item| item.name.name.eq_ignore_ascii_case("namespace"))
                    {
                        report(item.name.span, &item.name.name);
                    }
                }
            }
        }
        Stmt::Const { items, .. } => {
            for item in items
                .iter()
                .filter(|item| reserved(&item.name.name) || item.name.name == "_")
            {
                report(item.name.span, &item.name.name);
            }
        }
        Stmt::Use(items) => {
            for item in items {
                let name = crate::names::alias(item);
                if reserved(name) || (item.kind != UseKind::Function && name == "_") {
                    report(item.span, name);
                }
            }
        }
        _ => {}
    }
}

fn deprecated_call(context: &mut Context<'_>, node: Node<'_>) {
    if context.language != Language::Php86 {
        return;
    }
    let Node::Expression(expr, _) = node else {
        return;
    };
    let Expr::Call {
        callee,
        arguments: Arguments::List(args),
    } = &expr.kind
    else {
        return;
    };
    let Expr::Name(name) = &callee.kind else {
        return;
    };
    let message = if args.len() >= 3 && direct_function(context, name, "define") {
        "The third argument to define() is deprecated in PHP 8.6. Omit it."
    } else if let Some(first) = args.first()
        && matches!(first.value.kind, Expr::Literal(Literal::String(_)))
        && (direct_function(context, name, "is_a") && args.len() == 2
            || matches!(
                args.get(2).map(|arg| &arg.value.kind),
                Some(Expr::Literal(Literal::Bool(false)))
            ) && (direct_function(context, name, "is_a")
                || direct_function(context, name, "is_subclass_of")))
    {
        "Passing a class name with allow_string disabled is deprecated in PHP 8.6. Use an object or enable allow_string."
    } else {
        return;
    };
    context.report("php86-deprecated-call", expr.span, message);
}

fn deprecated_method(context: &mut Context<'_>, node: Node<'_>) {
    if context.language != Language::Php86 {
        return;
    }
    let Node::Method(method) = node else {
        return;
    };
    let name = method.name.name.to_ascii_lowercase();
    if name == "__debuginfo"
        && let Some(ty) = &method.return_type
    {
        let nullable = match &ty.kind {
            TypeKind::Nullable(_) => true,
            TypeKind::Union(arms) => arms.iter().any(|arm| matches!(&arm.kind, TypeKind::Named(name) if name.text.eq_ignore_ascii_case("null"))),
            _ => false,
        };
        if nullable {
            context.report(
                "php86-deprecated-magic-method",
                ty.span,
                "A nullable __debugInfo() return type is deprecated in PHP 8.6. Specify array.",
            );
        }
    }
    if !matches!(name.as_str(), "__construct" | "__destruct") {
        return;
    }
    if method.is_generator {
        context.report("php86-deprecated-magic-method", method.name.span, "Generator constructors and destructors are deprecated in PHP 8.6. Move the yield to a separate method.");
    }
    let Some(body) = &method.body else {
        return;
    };
    let mut pending = vec![body.as_slice()];
    while let Some(block) = pending.pop() {
        for statement in block {
            if matches!(statement.kind, Stmt::Return(Some(_))) {
                context.report("php86-deprecated-magic-method", statement.span, "Returning a value from this method is deprecated in PHP 8.6. Use a bare return.");
            }
            pending.extend(crate::collect::child_blocks(statement));
        }
    }
}
