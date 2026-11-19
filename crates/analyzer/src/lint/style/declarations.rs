//! Check member declarations such as `final public static function run()`.

use super::style;
use crate::lint::{
    Context,
    fix::{Edit, Fix},
    rule::{Rule, Visit},
    traversal::Node,
};
use phera_ast::{
    ClassMemberKind as Member, ExpressionKind as Expr, Literal, StatementKind as Stmt,
};
use phera_lexer::TokenKind as Kind;
use phera_source::Span;

pub(super) const RULES: &[Rule] = &[
    style(
        "one-statement-per-line",
        "Put consecutive statements on separate lines.",
        "echo 1; echo 2;",
        "echo 1;\necho 2;",
        "",
        Visit::Block,
        one_per_line,
    ),
    style(
        "explicit-visibility",
        "Declare member visibility explicitly.",
        "class C { function run() {} }",
        "class C { public function run() {} }",
        "",
        Visit::Class,
        visibility,
    ),
    style(
        "modifier-order",
        "Place final and abstract before visibility, then static and readonly.",
        "class C { public final function run() {} }",
        "class C { final public function run() {} }",
        "",
        Visit::Class,
        modifiers,
    ),
    style(
        "strict-types",
        "Declare strict_types=1 in PHP files when configured.",
        "echo 1;",
        "declare(strict_types=1);\necho 1;",
        "",
        Visit::File,
        strict_types,
    ),
    style(
        "import-order",
        "Keep adjacent imports in alphabetical order within each kind.",
        "use App\\Z;\nuse App\\A;",
        "use App\\A;\nuse App\\Z;",
        "",
        Visit::Block,
        imports,
    ),
    style(
        "import-leading-backslash",
        "Omit a redundant leading backslash in an import.",
        "use \\App\\Client;\nnew Client();",
        "use App\\Client;\nnew Client();",
        "Safe token fix.",
        Visit::Statement,
        leading_backslash,
    ),
];

fn one_per_line(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Block(block) = node else {
        return;
    };
    for pair in block.windows(2) {
        let [first, second] = pair else {
            continue;
        };
        if matches!(first.kind, Stmt::InlineHtml(_)) || matches!(second.kind, Stmt::InlineHtml(_)) {
            continue;
        }
        if context.lines.line(first.span.end.saturating_sub(1))
            == context.lines.line(second.span.start)
        {
            context.report(
                "one-statement-per-line",
                second.span,
                "Start this statement on a new line.",
            );
        }
    }
}

fn visibility(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Class(class) = node else {
        return;
    };
    for member in &class.members {
        let modifiers = match &member.kind {
            Member::Method(method) => method.modifiers,
            Member::Constant { modifiers, .. } | Member::Property { modifiers, .. } => *modifiers,
            _ => continue,
        };
        if modifiers.visibility.is_none() {
            context.report(
                "explicit-visibility",
                member.span,
                "Declare the visibility of this member.",
            );
        }
    }
}

fn modifiers(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Class(class) = node else {
        return;
    };
    for member in &class.members {
        if matches!(
            member.kind,
            Member::EnumCase { .. } | Member::TraitUse { .. }
        ) {
            continue;
        }
        let start = match &member.kind {
            Member::Property { attributes, .. } | Member::Constant { attributes, .. } => attributes
                .last()
                .map_or(member.span.start, |attr| attr.span.end),
            Member::Method(method) => method
                .attributes
                .last()
                .map_or(member.span.start, |attr| attr.span.end),
            _ => continue,
        };
        let order: Vec<_> = context
            .source
            .within(Span::new(start, member.span.end))
            .iter()
            .filter(|token| {
                !matches!(
                    token.kind,
                    Kind::Whitespace | Kind::Comment | Kind::DocComment | Kind::RightBracket
                )
            })
            .map_while(|token| match token.kind {
                Kind::Abstract | Kind::Final => Some(0u8),
                Kind::Public
                | Kind::Protected
                | Kind::Private
                | Kind::PublicSet
                | Kind::ProtectedSet
                | Kind::PrivateSet => Some(1),
                Kind::Static => Some(2),
                Kind::Readonly => Some(3),
                _ => None,
            })
            .collect();
        if order.windows(2).any(|pair| pair[0] > pair[1]) {
            context.report(
                "modifier-order",
                member.span,
                "Order the member modifiers consistently.",
            );
        }
    }
}

fn strict_types(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Block(body) = node else {
        return;
    };
    let Some(first) = body.first() else {
        return;
    };
    if matches!(first.kind, Stmt::InlineHtml(_)) {
        return;
    }
    if !body.iter().any(|statement| matches!(&statement.kind, Stmt::Declare { directives, .. }
        if directives.iter().any(|directive| directive.name.name.eq_ignore_ascii_case("strict_types")
            && matches!(directive.value.kind, Expr::Literal(Literal::Int(1))))))
    {
        context.report("strict-types", first.span, "Declare strict_types=1 at the beginning of this PHP file.");
    }
}

fn imports(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Block(body) = node else {
        return;
    };
    let mut previous: Option<(phera_ast::UseKind, String)> = None;
    for statement in body {
        let Stmt::Use(items) = &statement.kind else {
            previous = None;
            continue;
        };
        for item in items {
            let next = (item.kind, item.name.text.to_ascii_lowercase());
            if previous
                .as_ref()
                .is_some_and(|(kind, name)| *kind == next.0 && name > &next.1)
            {
                context.report(
                    "import-order",
                    item.span,
                    "Sort this import alphabetically within its kind.",
                );
            }
            previous = Some(next);
        }
    }
}

fn leading_backslash(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Statement(statement) = node else {
        return;
    };
    if !matches!(statement.kind, Stmt::Use(_)) {
        return;
    }
    let spans: Vec<_> = context
        .source
        .within(statement.span)
        .iter()
        .filter(|token| token.kind == Kind::FullyQualifiedName)
        .map(|token| Span::new(token.span.start, token.span.start + 1))
        .collect();
    for span in spans {
        context.with_fix(
            "import-leading-backslash",
            span,
            "Remove the leading backslash from this import.",
            Fix::safe(vec![Edit::remove(span)]),
        );
    }
}
