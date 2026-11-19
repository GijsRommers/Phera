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
        self.context.documented_return = None;
        self.context.generator =
            self.context.declared_return && generators::body_yields(&function.body);
        self.context.by_reference = function.by_reference;
        self.context.returns =
            ValueType::declared(function.return_type.as_ref(), &self.context.names);
        let mut scope = Scope::default();
        self.check_attributes(&function.attributes, &mut scope);
        self.parameter_scope(&function.parameters, &mut scope);
        if self.level >= 2 && function.doc_comment.is_some() {
            let signature = self.declaration_signature(
                &function.parameters,
                function.return_type.as_ref(),
                span,
            );
            if let Some(returns) =
                self.function_documentation(function.doc_comment, &signature, span, &mut scope)
            {
                self.context.documented_return = Some(returns);
            }
        }
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
        self.check_attributes(&class.attributes, &mut Scope::default());
        if self.level >= 2 && class.doc_comment.is_some() {
            self.read_documentation(class.doc_comment);
        }
        for name in class.extends.iter().chain(&class.implements) {
            self.class_name(name, true);
        }
        if let Some(name) = &self.context.class.clone() {
            self.required_methods(name, class.span);
        }
        let consumer_contexts = if class.kind == ClassKind::Trait {
            self.context
                .class
                .as_deref()
                .map_or_else(Vec::new, |name| self.index.trait_consumers(name))
        } else {
            Vec::new()
        };
        let declaring_class = self.context.class.clone();
        for member in &class.members {
            match &member.kind {
                ClassMemberKind::Method(method) => {
                    if class.kind != ClassKind::Trait {
                        self.method_declaration(method, member.span, member.doc_comment);
                    } else {
                        for consumer in &consumer_contexts {
                            self.context.class = Some(consumer.clone());
                            self.method_declaration(method, member.span, member.doc_comment);
                        }
                        self.context.class.clone_from(&declaring_class);
                    }
                }
                ClassMemberKind::Property {
                    ty,
                    items,
                    attributes,
                    ..
                } => {
                    self.check_attributes(attributes, &mut Scope::default());
                    self.check_type(ty.as_ref());
                    if self.level >= 2 && member.doc_comment.is_some() {
                        let docs = self.read_documentation(member.doc_comment);
                        let native = ValueType::declared(ty.as_ref(), &self.context.names);
                        for annotation in docs.variables.values() {
                            self.check_doc_annotation(
                                annotation,
                                &native,
                                member.span,
                                "property.phpDocType",
                                "@var",
                            );
                        }
                    }
                    for item in items {
                        if let Some(default) = &item.default {
                            self.expression(default, &mut Scope::default(), false);
                        }
                        if consumer_contexts.is_empty() {
                            self.property_hooks(&item.name, ty.as_ref(), &item.hooks);
                        } else {
                            for consumer in &consumer_contexts {
                                self.context.class = Some(consumer.clone());
                                self.property_hooks(&item.name, ty.as_ref(), &item.hooks);
                            }
                            self.context.class.clone_from(&declaring_class);
                        }
                    }
                }
                ClassMemberKind::Constant {
                    ty,
                    items,
                    attributes,
                    ..
                } => {
                    self.check_attributes(attributes, &mut Scope::default());
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
                ClassMemberKind::EnumCase {
                    value, attributes, ..
                } => {
                    self.check_attributes(attributes, &mut Scope::default());
                    if let Some(value) = value {
                        self.expression(value, &mut Scope::default(), false);
                    }
                }
            }
        }
        self.context = previous;
        self.exception_scopes = exceptions;
        self.controls = controls;
        self.return_scopes = returns;
    }

    fn method_declaration(&mut self, method: &Method, span: Span, doc: Option<Span>) {
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
        self.context.documented_return = None;
        self.context.generator = self.context.declared_return
            && method
                .body
                .as_ref()
                .is_some_and(|body| generators::body_yields(body));
        self.context.by_reference = method.by_reference;
        self.context.returns =
            ValueType::declared(method.return_type.as_ref(), &self.context.names);
        let mut scope = Scope::default();
        self.check_attributes(&method.attributes, &mut scope);
        self.parameter_scope(&method.parameters, &mut scope);
        if self.level >= 2 && doc.is_some() {
            let signature =
                self.declaration_signature(&method.parameters, method.return_type.as_ref(), span);
            if let Some(returns) = self.function_documentation(doc, &signature, span, &mut scope) {
                self.context.documented_return = Some(returns);
            }
        }
        self.check_type(method.return_type.as_ref());
        for parameter in &method.parameters {
            self.property_hooks(&parameter.name, parameter.ty.as_ref(), &parameter.hooks);
        }
        if let Some(body) = &method.body {
            let flow = self.block(body, &mut scope);
            self.check_return_path(flow, body, span);
        }
    }

    pub fn declaration_signature(
        &self,
        parameters: &[Parameter],
        returns: Option<&Type>,
        span: Span,
    ) -> crate::index::Signature {
        crate::index::Signature::new(
            parameters,
            returns,
            &self.context.names,
            crate::index::Location {
                file: self.file.path.clone(),
                line: self.file.lines.line(span.start),
            },
        )
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
            || self.context.generator
            || (self.context.returns == ValueType::Never && self.level == 0)
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
            }
        }
        let span = body.last().map_or(span, |statement| statement.span);
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
