//! `private int $count;` is write-only if code sets it and never reads it.
//! Dynamic member names and trait bodies can hide uses of private members.

use super::*;
use std::collections::BTreeSet;

#[derive(Default)]
struct Uses {
    methods: BTreeSet<String>,
    reads: BTreeSet<String>,
    writes: BTreeSet<String>,
    constants: BTreeSet<String>,
    forwarded_parameters: BTreeSet<String>,
    dynamic_method: bool,
    dynamic_property: bool,
    dynamic_constant: bool,
}

impl Analyzer<'_> {
    pub fn unused_private_members(&mut self, declaration: &ClassDeclaration) {
        if self.level < 4
            || !matches!(declaration.kind, ClassKind::Class | ClassKind::Enum)
            || declaration
                .members
                .iter()
                .any(|member| matches!(member.kind, ClassMemberKind::TraitUse { .. }))
        {
            return;
        }
        let Some(class) = self.context.class.clone() else {
            return;
        };
        if !self.index.complete(&class) {
            return;
        }
        let has_private = declaration.members.iter().any(|member| match &member.kind {
            ClassMemberKind::Method(method) => {
                is_private(method.modifiers)
                    || method
                        .parameters
                        .iter()
                        .any(|param| is_private(param.modifiers))
            }
            ClassMemberKind::Property { modifiers, .. }
            | ClassMemberKind::Constant { modifiers, .. } => is_private(*modifiers),
            _ => false,
        });
        if !has_private {
            return;
        }
        let uses = Uses::collect(declaration);
        for member in &declaration.members {
            match &member.kind {
                ClassMemberKind::Method(method) => {
                    let name = &method.name.name;
                    if is_private(method.modifiers)
                        && !name.starts_with("__")
                        && !uses.dynamic_method
                        && !uses.methods.contains(&name.to_ascii_lowercase())
                    {
                        let label = if method.modifiers.is_static {
                            "Static method"
                        } else {
                            "Method"
                        };
                        self.diagnose_dead_code(
                            "method.unused",
                            member.span,
                            format!("{label} {class}::{name}() is unused."),
                        );
                    }
                    for param in &method.parameters {
                        if is_private(param.modifiers)
                            && !uses.forwarded_parameters.contains(&param.name)
                        {
                            self.check_private_property(
                                &uses,
                                &class,
                                &param.name,
                                param.span,
                                false,
                                true,
                            );
                        }
                    }
                }
                ClassMemberKind::Property {
                    modifiers, items, ..
                } if is_private(*modifiers) => {
                    for item in items.iter().filter(|item| !item.has_hook_list) {
                        self.check_private_property(
                            &uses,
                            &class,
                            &item.name,
                            member.span,
                            modifiers.is_static,
                            item.default.is_some(),
                        );
                    }
                }
                ClassMemberKind::Constant {
                    modifiers, items, ..
                } if is_private(*modifiers) && !uses.dynamic_constant => {
                    for item in items
                        .iter()
                        .filter(|item| !uses.constants.contains(&item.name.name))
                    {
                        self.diagnose_dead_code(
                            "classConstant.unused",
                            member.span,
                            format!("Constant {class}::{} is unused.", item.name.name),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn check_private_property(
        &mut self,
        uses: &Uses,
        class: &str,
        name: &str,
        span: Span,
        is_static: bool,
        initialized: bool,
    ) {
        if uses.dynamic_property {
            return;
        }
        let (identifier, reason) = match (
            uses.reads.contains(name),
            initialized || uses.writes.contains(name),
        ) {
            (false, false) => ("property.unused", "unused"),
            (false, true) => ("property.onlyWritten", "never read, only written"),
            (true, false) => ("property.onlyRead", "never written, only read"),
            (true, true) => return,
        };
        let label = if is_static {
            "Static property"
        } else {
            "Property"
        };
        self.diagnose_dead_code(
            identifier,
            span,
            format!("{label} {class}::${name} is {reason}."),
        );
    }
}

impl Uses {
    fn collect(declaration: &ClassDeclaration) -> Self {
        let mut uses = Self::default();
        let mut statements = Vec::new();
        let mut expressions = Vec::new();
        for member in &declaration.members {
            match &member.kind {
                ClassMemberKind::Method(method) => {
                    statements.extend(method.body.iter().flatten());
                    expressions.extend(
                        method
                            .parameters
                            .iter()
                            .filter_map(|param| param.default.as_ref()),
                    );
                }
                ClassMemberKind::Property { items, .. } => {
                    for item in items {
                        expressions.extend(item.default.iter());
                        for hook in &item.hooks {
                            match &hook.body {
                                HookBody::Block(body) => statements.extend(body),
                                HookBody::Expression(expr) => expressions.push(expr),
                                HookBody::None => {}
                            }
                        }
                    }
                }
                ClassMemberKind::Constant { items, .. } => {
                    expressions.extend(items.iter().map(|item| &item.value))
                }
                _ => {}
            }
        }
        while !statements.is_empty() || !expressions.is_empty() {
            if let Some(statement) = statements.pop() {
                statements.extend(
                    crate::collect::child_blocks(statement)
                        .into_iter()
                        .flatten(),
                );
                expressions.extend(crate::walk::statement_expressions(statement));
                match &statement.kind {
                    StatementKind::Switch { cases, .. } => {
                        expressions.extend(cases.iter().filter_map(|case| case.condition.as_ref()));
                    }
                    StatementKind::Foreach { key, value, .. } => {
                        for target in key.iter().chain([value]) {
                            uses.property(target, PropertyUse::Write);
                            expressions.extend(crate::walk::children(target));
                        }
                    }
                    _ => {}
                }
                continue;
            }
            if let Some(expr) = expressions.pop() {
                match &expr.kind {
                    ExpressionKind::StaticCall {
                        class,
                        name: MemberName::Identifier(method),
                        arguments: Arguments::List(arguments),
                    } if matches!(&class.kind, ExpressionKind::Name(name) if name.text.eq_ignore_ascii_case("parent"))
                        && method.name.eq_ignore_ascii_case("__construct") =>
                    {
                        for argument in arguments {
                            if let ExpressionKind::Variable(Variable::Named(name)) =
                                &argument.value.kind
                            {
                                // The child may promote a parameter that also
                                // initializes the parent implementation.
                                uses.forwarded_parameters.insert(name.clone());
                            }
                        }
                    }
                    ExpressionKind::Assign { target, value }
                    | ExpressionKind::AssignReference { target, value } => {
                        uses.property(target, PropertyUse::Write);
                        expressions.push(value);
                        // Evaluate the receiver and key without reading the written property.
                        expressions.extend(crate::walk::children(target));
                        continue;
                    }
                    ExpressionKind::CompoundAssign { target, .. }
                    | ExpressionKind::IncDec { target, .. } => {
                        uses.property(target, PropertyUse::Write);
                    }
                    ExpressionKind::Property { .. } | ExpressionKind::StaticProperty { .. } => {
                        uses.property(expr, PropertyUse::Read);
                    }
                    ExpressionKind::MethodCall { name, .. }
                    | ExpressionKind::StaticCall { name, .. } => match name {
                        MemberName::Identifier(name) => {
                            uses.methods.insert(name.name.to_ascii_lowercase());
                        }
                        MemberName::Expression(_) => uses.dynamic_method = true,
                    },
                    ExpressionKind::ClassConstant { name, .. } => match name {
                        MemberName::Identifier(name) => {
                            uses.constants.insert(name.name.clone());
                        }
                        MemberName::Expression(_) => uses.dynamic_constant = true,
                    },
                    ExpressionKind::Closure(closure) => statements.extend(&closure.body),
                    ExpressionKind::ArrowFunction(arrow) => expressions.push(&arrow.body),
                    ExpressionKind::Literal(Literal::String(bytes)) => {
                        // In `[$this, 'run']` the method call can be outside the class.
                        if let Ok(name) = std::str::from_utf8(bytes) {
                            uses.methods.insert(name.to_ascii_lowercase());
                        }
                    }
                    ExpressionKind::Include { .. } | ExpressionKind::Eval(_) => {
                        uses.dynamic_method = true;
                        uses.dynamic_property = true;
                        uses.dynamic_constant = true;
                    }
                    _ => {}
                }
                expressions.extend(crate::walk::children(expr));
            }
        }
        uses
    }

    fn property(&mut self, expression: &Expression, use_kind: PropertyUse) {
        let name = match &expression.kind {
            ExpressionKind::Property {
                name: MemberName::Identifier(name),
                ..
            } => &name.name,
            ExpressionKind::StaticProperty {
                name: Variable::Named(name),
                ..
            } => name,
            ExpressionKind::Property { .. } | ExpressionKind::StaticProperty { .. } => {
                self.dynamic_property = true;
                return;
            }
            ExpressionKind::Index { .. } | ExpressionKind::List(_) | ExpressionKind::Array(_)
                if use_kind == PropertyUse::Write =>
            {
                // A write such as `$this->items[] = $item` writes its root.
                let mut pending = vec![expression];
                while let Some(expression) = pending.pop() {
                    match &expression.kind {
                        ExpressionKind::Index { target, .. } => pending.push(target),
                        ExpressionKind::List(items) => {
                            pending.extend(items.iter().flatten().map(|item| &item.value))
                        }
                        ExpressionKind::Array(items) => {
                            pending.extend(items.iter().map(|item| &item.value))
                        }
                        ExpressionKind::Property { .. } | ExpressionKind::StaticProperty { .. } => {
                            self.dynamic_property = true;
                        }
                        _ => {}
                    }
                }
                return;
            }
            _ => return,
        };
        match use_kind {
            PropertyUse::Read => self.reads.insert(name.clone()),
            PropertyUse::Write => self.writes.insert(name.clone()),
        };
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PropertyUse {
    Read,
    Write,
}

fn is_private(modifiers: Modifiers) -> bool {
    modifiers.visibility == Some(Visibility::Private)
}
