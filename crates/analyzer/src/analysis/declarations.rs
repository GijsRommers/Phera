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
                        name.text.to_ascii_lowercase().as_str(),
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
        if !self.checked_declarations.insert(span.start) {
            return;
        }
        let previous = self.context.clone();
        let exceptions = std::mem::take(&mut self.exception_scopes);
        let controls = std::mem::take(&mut self.controls);
        let returns = std::mem::take(&mut self.return_scopes);
        self.context.scope_kind = ScopeKind::Function;
        self.context.incomplete = false;
        self.context.class = None;
        self.context.this_class = None;
        self.context.object = false;
        self.context.unbound_this = None;
        self.context.function = self.context.names.declaration(&function.name.name);
        self.context.declared_return = function.return_type.is_some();
        self.context.by_reference = function.by_reference;
        self.context.returns =
            ValueType::declared(function.return_type.as_ref(), &self.context.names);
        let mut scope = Scope::default();
        self.parameter_scope(&function.parameters, &mut scope);
        self.check_type(function.return_type.as_ref());
        let flow = self.block(&function.body, &mut scope);
        self.check_return_path(flow, &function.body, span);
        self.context = previous;
        self.exception_scopes = exceptions;
        self.controls = controls;
        self.return_scopes = returns;
    }

    pub fn class_declaration(&mut self, class: &ClassDeclaration) {
        if !self.checked_declarations.insert(class.span.start) {
            return;
        }
        let previous = self.context.clone();
        let exceptions = std::mem::take(&mut self.exception_scopes);
        let controls = std::mem::take(&mut self.controls);
        let returns = std::mem::take(&mut self.return_scopes);
        self.context.class = Some(class.name.as_ref().map_or_else(
            || crate::collect::anonymous_name(&self.file.path, class.span.start),
            |n| self.context.names.declaration(&n.name),
        ));
        for name in class.extends.iter().chain(&class.implements) {
            self.class_name(name, true);
        }
        if let Some(name) = &self.context.class.clone() {
            self.required_methods(name, class.span);
        }
        for member in &class.members {
            match &member.kind {
                ClassMemberKind::Method(method) => self.method_declaration(method, member.span),
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
        self.exception_scopes = exceptions;
        self.controls = controls;
        self.return_scopes = returns;
    }

    fn method_declaration(&mut self, method: &Method, span: Span) {
        self.context.scope_kind = ScopeKind::Function;
        self.context.incomplete = false;
        self.context.object = !method.modifiers.is_static;
        self.context.unbound_this = None;
        self.context.this_class = if self.context.object {
            self.context.class.clone()
        } else {
            None
        };
        self.context.function = method.name.name.clone();
        self.context.declared_return = method.return_type.is_some();
        self.context.by_reference = method.by_reference;
        self.context.returns =
            ValueType::declared(method.return_type.as_ref(), &self.context.names);
        let mut scope = Scope::default();
        self.parameter_scope(&method.parameters, &mut scope);
        self.check_type(method.return_type.as_ref());
        if let Some(body) = &method.body {
            let flow = self.block(body, &mut scope);
            self.check_return_path(flow, body, span);
        }
    }

    fn required_methods(&mut self, name: &str, span: Span) {
        let Some(class) = self.index.class(name) else {
            return;
        };
        if !matches!(class.kind, ClassKind::Class | ClassKind::Enum)
            || class.modifiers.is_abstract
            || !self.index.complete(name)
        {
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
        if self.context.returns == ValueType::Void {
            self.void_returns(body);
            return;
        }
        if self.context.incomplete
            || flow != Flow::Next
            || !self.context.declared_return
            || self.context.returns == ValueType::Void
        {
            return;
        }
        let mut blocks = vec![body];
        while let Some(block) = blocks.pop() {
            for statement in block {
                // Syntactic fallthrough cannot prove that a goto body is
                // missing a return. Each explicit return is still checked.
                if matches!(statement.kind, StatementKind::Goto(_)) {
                    return;
                }
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
        let span = body.first().map_or(span, |statement| statement.span);
        self.report(Finding::MissingReturn(self.context.function.clone()), span);
    }
    fn void_returns(&mut self, body: &[Statement]) {
        let mut pending = vec![body];
        while let Some(block) = pending.pop() {
            for statement in block {
                if matches!(statement.kind, StatementKind::Return(Some(_))) {
                    self.report(Finding::VoidReturn, statement.span);
                }
                pending.extend(crate::collect::child_blocks(statement));
            }
        }
    }
}
