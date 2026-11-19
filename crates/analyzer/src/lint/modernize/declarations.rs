//! Declaration modernization includes `class Counter { private const int STEP = 1; }`.

use super::rule;
use crate::lint::{
    Context,
    rule::{Rule, Visit},
    traversal::Node,
};
use phera_ast::{
    ClassDeclaration, ClassMemberKind as Member, ExpressionKind as Expr, Literal, MemberName,
    Method, StatementKind as Stmt, UseKind, Variable, Visibility,
};

pub(super) const RULES: &[Rule] = &[
    rule(
        "constructor-property-promotion",
        "Consider promoting parameters assigned directly to matching properties.",
        "class C { private int $n; function __construct(int $n) { $this->n = $n; } }",
        "class C { function __construct(private int $n) {} }",
        "Review attributes, reflection and declaration order before changing the signature.",
        Visit::Class,
        promote,
    ),
    rule(
        "readonly-property",
        "Consider readonly for a typed private property written only by its constructor.",
        "class C { private int $n; function __construct() { $this->n = 1; } }",
        "class C { private readonly int $n; function __construct() { $this->n = 1; } }",
        "Review serialization, cloning and reflection before adding readonly.",
        Visit::Class,
        readonly,
    ),
    rule(
        "typed-class-constant",
        "Consider a type on private class constants with scalar values.",
        "class C { private const N = 1; }",
        "class C { private const int N = 1; }",
        "",
        Visit::Class,
        typed_constant,
    ),
    rule(
        "override-attribute",
        "Mark methods that override a parent or interface declaration with #[\\Override].",
        "class P { function run() {} } class C extends P { function run() {} }",
        "class P { function run() {} } class C extends P { #[\\Override] function run() {} }",
        "",
        Visit::Class,
        override_attribute,
    ),
];

fn promote(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Class(class) = node else {
        return;
    };
    let Some(constructor) = constructor(class) else {
        return;
    };
    let Some(body) = &constructor.body else {
        return;
    };
    for statement in body {
        let Stmt::Expression(expr) = &statement.kind else {
            break;
        };
        let Expr::Assign { target, value } = &expr.kind else {
            break;
        };
        let (Some(property), Expr::Variable(Variable::Named(parameter))) =
            (this_property(target), &value.kind)
        else {
            break;
        };
        if property.name != *parameter {
            break;
        }
        let Some(param) = constructor
            .parameters
            .iter()
            .find(|candidate| candidate.name == *parameter)
        else {
            break;
        };
        if param.is_promoted()
            || param.variadic
            || param.by_reference
            || !param.attributes.is_empty()
        {
            continue;
        }
        for member in &class.members {
            if let Member::Property {
                modifiers,
                items,
                attributes,
                ty,
                ..
            } = &member.kind
                && modifiers.visibility.is_some()
                && !modifiers.is_static
                && attributes.is_empty()
                && items.iter().any(|item| {
                    item.name == *parameter && item.default.is_none() && !item.has_hook_list
                })
                && ty.as_ref().map(|ty| context.source.slice(ty.span))
                    == param.ty.as_ref().map(|ty| context.source.slice(ty.span))
            {
                context.report(
                    "constructor-property-promotion",
                    param.span,
                    format!("Consider promoting ${parameter} in the constructor."),
                );
            }
        }
    }
}

fn readonly(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Class(class) = node else {
        return;
    };
    if class.modifiers.is_readonly
        || class
            .members
            .iter()
            .any(|member| matches!(member.kind, Member::TraitUse { .. }))
    {
        return;
    }
    let Some(constructor) = constructor(class) else {
        return;
    };
    let Some(body) = &constructor.body else {
        return;
    };
    let mut writes = std::collections::HashMap::new();
    for statement in body {
        if let Stmt::Expression(expr) = &statement.kind
            && let Expr::Assign { target, .. } = &expr.kind
            && let Some(name) = this_property(target)
        {
            *writes.entry(name.name.as_str()).or_insert(0usize) += 1;
        }
    }
    for member in &class.members {
        if let Member::Property {
            modifiers,
            items,
            ty: Some(_),
            attributes,
            ..
        } = &member.kind
            && !modifiers.is_static
            && !modifiers.is_readonly
            && modifiers.visibility == Some(Visibility::Private)
            && attributes.is_empty()
        {
            for item in items {
                if item.name.is_empty()
                    || item.default.is_some()
                    || item.has_hook_list
                    || writes.get(item.name.as_str()) != Some(&1)
                {
                    continue;
                }
                // A reference to another object's private slot can appear in
                // any method. Leave properties with other references alone.
                let references_in_class = context
                    .source
                    .within(class.span)
                    .iter()
                    .filter(|token| {
                        let bytes = context.source.slice(token.span);
                        bytes.strip_prefix(b"$").unwrap_or(bytes) == item.name.as_bytes()
                    })
                    .take(3)
                    .count();
                if references_in_class == 2 {
                    context.report(
                        "readonly-property",
                        item.span,
                        format!(
                            "Consider readonly for ${}, assigned once in the constructor.",
                            item.name
                        ),
                    );
                }
            }
        }
    }
}

fn typed_constant(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Class(class) = node else {
        return;
    };
    for member in &class.members {
        if let Member::Constant {
            ty: None,
            modifiers,
            items,
            ..
        } = &member.kind
            && modifiers.visibility == Some(Visibility::Private)
        {
            for item in items {
                let ty = match &item.value.kind {
                    Expr::Literal(Literal::Int(_)) => "int",
                    Expr::Literal(Literal::Float(_)) => "float",
                    Expr::Literal(Literal::Bool(_)) => "bool",
                    Expr::Literal(Literal::String(_)) => "string",
                    _ => continue,
                };
                context.report(
                    "typed-class-constant",
                    item.name.span,
                    format!("Consider declaring '{}' as {ty}.", item.name.name),
                );
            }
        }
    }
}

fn override_attribute(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Class(class) = node else {
        return;
    };
    let Some(index) = context.index else {
        return;
    };
    let supers: Vec<_> = class
        .extends
        .iter()
        .chain(&class.implements)
        .filter_map(|parent| {
            context
                .names
                .resolve(parent, UseKind::Class)
                .into_iter()
                .next()
        })
        .collect();
    if supers.is_empty() {
        return;
    }
    for member in &class.members {
        let Member::Method(method) = &member.kind else {
            continue;
        };
        if method.name.name.starts_with("__")
            || method.modifiers.visibility == Some(Visibility::Private)
        {
            continue;
        }
        let inherited = supers.iter().any(|parent| {
            index
                .method(parent, &method.name.name)
                .is_some_and(|method| method.modifiers.visibility != Some(Visibility::Private))
        });
        if inherited
            && !method.attributes.iter().any(|attr| {
                context
                    .names
                    .resolve(&attr.name, UseKind::Class)
                    .first()
                    .is_some_and(|name| name.eq_ignore_ascii_case("Override"))
            })
        {
            context.report(
                "override-attribute",
                method.name.span,
                format!("Add #[\\Override] to '{}'.", method.name.name),
            );
        }
    }
}

fn this_property(expression: &phera_ast::Expression) -> Option<&phera_ast::Identifier> {
    let Expr::Property {
        target,
        name: MemberName::Identifier(name),
        nullsafe: false,
    } = &expression.kind
    else {
        return None;
    };
    matches!(&target.kind, Expr::Variable(Variable::Named(value)) if value == "this")
        .then_some(name)
}

fn constructor(class: &ClassDeclaration) -> Option<&Method> {
    class.members.iter().find_map(|member| match &member.kind {
        Member::Method(method) if method.name.name.eq_ignore_ascii_case("__construct") => {
            Some(method)
        }
        _ => None,
    })
}
