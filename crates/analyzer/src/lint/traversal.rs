//! Walk every syntax context, including `#[A] function f(C $value): D {}`.
//! The work stack carries the enclosing callable, class and finally region.

use crate::{collect::child_blocks, walk};
use phera_ast::*;

#[derive(Clone, Copy)]
pub(super) enum Node<'a> {
    Block(&'a [Statement]),
    Statement(&'a Statement),
    Expression(&'a Expression, UseKind),
    Class(&'a ClassDeclaration),
    Method(&'a Method),
    Name(&'a Name, UseKind),
    Type(&'a Type),
    Parameters(&'a [Parameter]),
    Hook(&'a PropertyHook),
    Attributes(&'a [Attribute]),
}

#[derive(Clone, Copy, Default)]
pub(super) struct Scope<'a> {
    pub class: Option<&'a ClassDeclaration>,
    pub callable: Option<usize>,
    pub finally: bool,
}

impl<'a> Scope<'a> {
    fn enter_callable(self, expression_or_declaration: usize) -> Self {
        Self {
            callable: Some(expression_or_declaration),
            finally: false,
            ..self
        }
    }
}

pub(super) fn visit<'a>(statements: &'a [Statement], mut inspect: impl FnMut(Node<'a>, Scope<'a>)) {
    let mut pending = vec![(Node::Block(statements), Scope::default())];
    while let Some((node, scope)) = pending.pop() {
        inspect(node, scope);
        let mut push = |node| pending.push((node, scope));
        match node {
            Node::Block(body) => {
                pending.extend(body.iter().rev().map(|s| (Node::Statement(s), scope)));
            }
            Node::Statement(statement) => visit_statement(statement, scope, &mut pending),
            Node::Expression(expr, kind) => visit_expression(expr, kind, scope, &mut pending),
            Node::Class(class) => visit_class(class, &mut pending),
            Node::Method(method) => {
                push(Node::Attributes(&method.attributes));
                visit_signature(&method.parameters, method.return_type.as_ref(), &mut push);
                if let Some(body) = &method.body {
                    pending.push((Node::Block(body), scope.enter_callable(method.span.start)));
                }
            }
            Node::Hook(hook) => {
                push(Node::Attributes(&hook.attributes));
                push(Node::Parameters(&hook.parameters));
                let body = match &hook.body {
                    HookBody::Block(body) => Node::Block(body),
                    HookBody::Expression(expr) => expression(expr),
                    HookBody::None => continue,
                };
                pending.push((body, scope.enter_callable(hook.span.start)));
            }
            Node::Parameters(parameters) => {
                for parameter in parameters {
                    push(Node::Attributes(&parameter.attributes));
                    if let Some(ty) = &parameter.ty {
                        push(Node::Type(ty));
                    }
                    if let Some(default) = &parameter.default {
                        push(expression(default));
                    }
                    for hook in &parameter.hooks {
                        push(Node::Hook(hook));
                    }
                }
            }
            Node::Attributes(attributes) => {
                for attr in attributes {
                    push(Node::Name(&attr.name, UseKind::Class));
                    for arg in &attr.arguments {
                        push(expression(&arg.value));
                    }
                }
            }
            Node::Type(ty) => match &ty.kind {
                TypeKind::Named(name) => push(Node::Name(name, UseKind::Class)),
                TypeKind::Nullable(inner) => push(Node::Type(inner)),
                TypeKind::Union(types) | TypeKind::Intersection(types) => {
                    for ty in types {
                        push(Node::Type(ty));
                    }
                }
            },
            Node::Name(_, _) => {}
        }
    }
}

type Pending<'a> = Vec<(Node<'a>, Scope<'a>)>;

fn visit_statement<'a>(s: &'a Statement, scope: Scope<'a>, pending: &mut Pending<'a>) {
    let mut push = |node| pending.push((node, scope));
    for expr in walk::statement_expressions(s) {
        push(expression(expr));
    }
    match &s.kind {
        StatementKind::Function(function) => {
            push(Node::Attributes(&function.attributes));
            visit_signature(
                &function.parameters,
                function.return_type.as_ref(),
                &mut push,
            );
            pending.push((
                Node::Block(&function.body),
                scope.enter_callable(function.span.start),
            ));
        }
        StatementKind::Class(class) => push(Node::Class(class)),
        StatementKind::Const { attributes, .. } => push(Node::Attributes(attributes)),
        StatementKind::Foreach { key, value, .. } => {
            if let Some(key) = key {
                push(expression(key));
            }
            push(expression(value));
        }
        StatementKind::Declare { directives, .. } => {
            for directive in directives {
                push(expression(&directive.value));
            }
        }
        StatementKind::Switch { cases, .. } => {
            for case in cases {
                if let Some(condition) = &case.condition {
                    push(expression(condition));
                }
            }
        }
        StatementKind::Global(variables) => {
            for variable in variables {
                if let Variable::Dynamic(expr) = variable {
                    push(expression(expr));
                }
            }
        }
        StatementKind::Try {
            body,
            catches,
            finally,
        } => {
            push(Node::Block(body));
            for catch in catches {
                for ty in &catch.types {
                    push(Node::Name(ty, UseKind::Class));
                }
                push(Node::Block(&catch.body));
            }
            if let Some(body) = finally {
                pending.push((
                    Node::Block(body),
                    Scope {
                        finally: true,
                        ..scope
                    },
                ));
            }
            return;
        }
        _ => {}
    }
    pending.extend(
        child_blocks(s)
            .into_iter()
            .map(|body| (Node::Block(body), scope)),
    );
}

fn visit_expression<'a>(
    expr: &'a Expression,
    kind: UseKind,
    scope: Scope<'a>,
    pending: &mut Pending<'a>,
) {
    let mut push = |node| pending.push((node, scope));
    let name_child = match &expr.kind {
        ExpressionKind::Call { callee, .. } => Some((callee.as_ref(), UseKind::Function)),
        ExpressionKind::New {
            class: NewTarget::Class(class),
            ..
        }
        | ExpressionKind::StaticCall { class, .. }
        | ExpressionKind::ClassConstant { class, .. }
        | ExpressionKind::StaticProperty { class, .. }
        | ExpressionKind::InstanceOf { class, .. } => Some((class.as_ref(), UseKind::Class)),
        _ => None,
    };
    for child in walk::children(expr) {
        let kind = name_child
            .filter(|(name, _)| std::ptr::eq(*name, child))
            .map_or(UseKind::Constant, |(_, kind)| kind);
        push(Node::Expression(child, kind));
    }
    match &expr.kind {
        ExpressionKind::Name(name) => push(Node::Name(name, kind)),
        ExpressionKind::New {
            class: NewTarget::Anonymous(class),
            ..
        } => push(Node::Class(class)),
        ExpressionKind::Closure(closure) => {
            push(Node::Attributes(&closure.attributes));
            visit_signature(&closure.parameters, closure.return_type.as_ref(), &mut push);
            pending.push((
                Node::Block(&closure.body),
                scope.enter_callable(expr.span.start),
            ));
        }
        ExpressionKind::ArrowFunction(arrow) => {
            push(Node::Attributes(&arrow.attributes));
            visit_signature(&arrow.parameters, arrow.return_type.as_ref(), &mut push);
            pending.push((
                expression(&arrow.body),
                scope.enter_callable(expr.span.start),
            ));
        }
        _ => {}
    }
}

fn visit_signature<'a>(
    parameters: &'a [Parameter],
    return_type: Option<&'a Type>,
    push: &mut impl FnMut(Node<'a>),
) {
    push(Node::Parameters(parameters));
    if let Some(ty) = return_type {
        push(Node::Type(ty));
    }
}

fn visit_class<'a>(class: &'a ClassDeclaration, pending: &mut Pending<'a>) {
    let scope = Scope {
        class: Some(class),
        ..Scope::default()
    };
    let mut push = |node| pending.push((node, scope));
    push(Node::Attributes(&class.attributes));
    for name in class.extends.iter().chain(&class.implements) {
        push(Node::Name(name, UseKind::Class));
    }
    if let Some(ty) = &class.backing_type {
        push(Node::Type(ty));
    }
    for member in &class.members {
        match &member.kind {
            ClassMemberKind::Method(method) => push(Node::Method(method)),
            ClassMemberKind::Property {
                attributes,
                ty,
                items,
                ..
            } => {
                push(Node::Attributes(attributes));
                if let Some(ty) = ty {
                    push(Node::Type(ty));
                }
                for item in items {
                    if let Some(default) = &item.default {
                        push(expression(default));
                    }
                    for hook in &item.hooks {
                        push(Node::Hook(hook));
                    }
                }
            }
            ClassMemberKind::Constant {
                attributes,
                ty,
                items,
                ..
            } => {
                push(Node::Attributes(attributes));
                if let Some(ty) = ty {
                    push(Node::Type(ty));
                }
                for item in items {
                    push(expression(&item.value));
                }
            }
            ClassMemberKind::EnumCase {
                attributes, value, ..
            } => {
                push(Node::Attributes(attributes));
                if let Some(value) = value {
                    push(expression(value));
                }
            }
            ClassMemberKind::TraitUse {
                traits,
                adaptations,
            } => {
                for name in traits {
                    push(Node::Name(name, UseKind::Class));
                }
                for adaptation in adaptations {
                    match adaptation {
                        TraitAdaptation::Precedence {
                            trait_name,
                            instead_of,
                            ..
                        } => {
                            push(Node::Name(trait_name, UseKind::Class));
                            for name in instead_of {
                                push(Node::Name(name, UseKind::Class));
                            }
                        }
                        TraitAdaptation::Alias {
                            trait_name: Some(name),
                            ..
                        } => {
                            push(Node::Name(name, UseKind::Class));
                        }
                        TraitAdaptation::Alias { .. } => {}
                    }
                }
            }
        }
    }
}

fn expression(expr: &Expression) -> Node<'_> {
    Node::Expression(expr, UseKind::Constant)
}
