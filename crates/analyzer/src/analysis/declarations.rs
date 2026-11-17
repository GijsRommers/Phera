//! Declaration checks: `class C implements Countable {}` lacks `count()`.
//! Bodies get fresh parameter scopes; traits defer consumer-dependent member facts.

use super::statements::Flow;
use super::*;

impl Analyzer<'_> {
    pub fn check_type(&mut self, ty: Option<&Type>) {
        let mut pending: Vec<_> = ty.into_iter().collect();
        while let Some(ty) = pending.pop() {
            match &ty.kind {
                TypeKind::Named(name) => {
                    if !matches!(
                        name.text.to_lowercase().as_str(),
                        "int"
                            | "float"
                            | "string"
                            | "bool"
                            | "true"
                            | "false"
                            | "null"
                            | "array"
                            | "iterable"
                            | "object"
                            | "mixed"
                            | "void"
                            | "never"
                            | "callable"
                            | "self"
                            | "static"
                            | "parent"
                    ) {
                        self.class_name(name, true);
                    }
                }
                TypeKind::Nullable(inner) => pending.push(inner),
                TypeKind::Union(items) | TypeKind::Intersection(items) => pending.extend(items),
            }
        }
    }

    pub fn function(&mut self, function: &FunctionDeclaration, span: Span) {
        let previous = self.context.clone();
        self.context.class = None;
        self.context.object = false;
        self.context.function = self.context.names.declaration(&function.name.name);
        self.context.returns =
            ValueType::declared(function.return_type.as_ref(), &self.context.names);
        let mut scope = Scope::default();
        self.parameter_scope(&function.parameters, &mut scope);
        self.check_type(function.return_type.as_ref());
        let flow = self.block(&function.body, &mut scope);
        self.check_return_path(flow, &function.body, span);
        self.context = previous;
    }

    pub fn class_declaration(&mut self, class: &ClassDeclaration) {
        let previous = self.context.clone();
        self.context.class = class
            .name
            .as_ref()
            .map(|n| self.context.names.declaration(&n.name));
        for name in class.extends.iter().chain(&class.implements) {
            self.class_name(name, true);
        }
        if let Some(name) = &self.context.class.clone() {
            self.required_methods(name, class.span);
        }
        for member in &class.members {
            match &member.kind {
                ClassMemberKind::Method(method) => {
                    self.context.object = !method.modifiers.is_static;
                    self.context.function = method.name.name.clone();
                    self.context.returns =
                        ValueType::declared(method.return_type.as_ref(), &self.context.names);
                    let mut scope = Scope::default();
                    self.parameter_scope(&method.parameters, &mut scope);
                    self.check_type(method.return_type.as_ref());
                    if let Some(body) = &method.body {
                        let flow = self.block(body, &mut scope);
                        self.check_return_path(flow, body, member.span);
                    }
                }
                ClassMemberKind::Property { ty, items, .. } => {
                    self.check_type(ty.as_ref());
                    for item in items {
                        if let Some(default) = &item.default {
                            self.expression(default, &mut Scope::default(), false);
                        }
                    }
                }
                ClassMemberKind::Constant { ty, items, .. } => {
                    self.check_type(ty.as_ref());
                    for item in items {
                        self.expression(&item.value, &mut Scope::default(), false);
                    }
                }
                ClassMemberKind::TraitUse { traits, .. } => {
                    for name in traits {
                        self.class_name(name, true);
                    }
                }
                _ => {}
            }
        }
        self.context = previous;
    }

    fn required_methods(&mut self, name: &str, span: Span) {
        let Some(class) = self.index.class(name) else {
            return;
        };
        if class.kind != ClassKind::Class || class.modifiers.is_abstract {
            return;
        }
        let mut missing = std::collections::BTreeSet::new();
        for ancestor in self.index.ancestry(name) {
            for (method, signature) in &ancestor.methods {
                if (ancestor.kind == ClassKind::Interface
                    || signature.abstract_body
                    || signature.modifiers.is_abstract)
                    && self
                        .index
                        .method(name, method)
                        .is_none_or(|m| m.abstract_body || m.modifiers.is_abstract)
                {
                    missing.insert(method.clone());
                }
            }
        }
        for method in missing {
            self.report(Finding::MissingMethod(name.into(), method), span);
        }
    }

    pub fn check_return_path(&mut self, flow: Flow, body: &[Statement], span: Span) {
        if flow != Flow::Next || matches!(self.context.returns, ValueType::Mixed | ValueType::Void)
        {
            return;
        }
        let mut blocks = vec![body];
        while let Some(block) = blocks.pop() {
            for statement in block {
                blocks.extend(crate::collect::child_blocks(statement));
                for e in crate::walk::statement_expressions(statement) {
                    let mut expressions = vec![e];
                    while let Some(e) = expressions.pop() {
                        if matches!(
                            e.kind,
                            ExpressionKind::Yield { .. } | ExpressionKind::YieldFrom(_)
                        ) {
                            return;
                        }
                        expressions.extend(crate::walk::children(e));
                    }
                }
            }
        }
        self.report(Finding::MissingReturn(self.context.function.clone()), span);
    }
}
