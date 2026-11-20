//! `/** @param User $u */ function f($u)` makes $u a User inside f.
//! A valid annotation refines its native type. Diagnostics point at the tag or declaration.

use super::*;
use crate::{
    phpdoc::{Annotation, Documentation},
    types::{Compatibility, TypePolicy},
};

impl Analyzer<'_> {
    pub fn function_documentation(
        &mut self,
        doc: Option<Span>,
        signature: &crate::index::Signature,
        declaration: Span,
        scope: &mut Scope,
    ) -> Option<ValueType> {
        if !self.index.read_type_docs || doc.is_none() {
            return None;
        }
        let docs = self.read_documentation(doc);
        for (name, annotated) in &docs.params {
            let Some(parameter) = signature
                .parameters
                .iter()
                .find(|parameter| parameter.name == *name)
            else {
                if self.level >= 2 {
                    self.diagnose(
                        "parameter.notFound",
                        declaration,
                        format!("PHPDoc tag @param references unknown parameter: ${name}"),
                    );
                }
                continue;
            };
            if self.check_doc_annotation(
                annotated,
                &parameter.ty,
                declaration,
                "parameter.phpDocType",
                &format!("@param for ${name}"),
            ) {
                let mut ty = crate::signature_types::declaration_refinement(
                    &parameter.ty,
                    annotated.ty.clone(),
                );
                if parameter.default_is_null {
                    ty = ty.join(&ValueType::Null);
                }
                if parameter.variadic {
                    ty = ValueType::Collection(std::sync::Arc::new(crate::types::Collection {
                        kind: crate::types::CollectionKind::Array,
                        key: ValueType::Int.join(&ValueType::String),
                        value: ty,
                        non_empty: false,
                    }));
                }
                if let Some(class) = self.context.class.as_deref() {
                    if self.level >= 5 {
                        let receiver = ValueType::Class(class.into());
                        ty = ty.substitute(
                            &Default::default(),
                            Some(crate::types::RelativeTypes {
                                receiver: &receiver,
                                owner: class,
                                parent: self
                                    .index
                                    .class(class)
                                    .and_then(|class| class.parents.first())
                                    .map(String::as_str),
                            }),
                        );
                    } else {
                        ty = self.relative_type(ty, class);
                    }
                }
                scope.define(name, ty);
            }
        }
        docs.returns.and_then(|annotated| {
            self.check_doc_annotation(
                &annotated,
                &signature.returns,
                declaration,
                "return.phpDocType",
                "@return",
            )
            .then(|| {
                crate::signature_types::declaration_refinement(&signature.returns, annotated.ty)
            })
        })
    }

    pub fn read_documentation(&mut self, doc: Option<Span>) -> Documentation {
        let empty = Default::default();
        let templates = self
            .context
            .documentation_class
            .as_ref()
            .and_then(|class| self.index.class(class))
            .map_or(&empty, |class| &class.documented.templates);
        let docs = crate::phpdoc::read(&self.file.source, doc, &self.context.names, templates);
        if self.level >= 2 {
            for invalid in &docs.errors {
                self.diagnose("phpDoc.parseError", invalid.span, invalid.message.clone());
            }
        }
        docs
    }

    pub fn check_doc_annotation(
        &mut self,
        annotation: &Annotation,
        native: &ValueType,
        span: Span,
        identifier: &str,
        tag: &str,
    ) -> bool {
        if annotation.ty == ValueType::Mixed {
            return false;
        }
        if annotation.ty == ValueType::ExplicitMixed && native.is_mixed() {
            return true;
        }
        if self.level < 2 {
            return native.accepts(&annotation.ty, self.index, TypePolicy::Documentation)
                == Compatibility::Accepts;
        }
        let known = self.check_doc_classes(&annotation.ty, span);
        let expected = self.context.class.as_ref().map_or_else(
            || native.clone(),
            |class| self.relative_type(native.clone(), class),
        );
        let given = self.context.class.as_ref().map_or_else(
            || annotation.ty.clone(),
            |class| self.relative_type(annotation.ty.clone(), class),
        );
        if expected.accepts(&given, self.index, TypePolicy::Documentation) == Compatibility::Rejects
        {
            self.diagnose(
                identifier,
                span,
                format!(
                    "PHPDoc tag {tag} with type {} is incompatible with native type {native}.",
                    annotation.ty
                ),
            );
            return false;
        }
        known
    }

    pub fn check_doc_classes(&mut self, ty: &ValueType, span: Span) -> bool {
        let mut known = true;
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            if let Some(class) = ty.class_name()
                && !["self", "static", "parent"].contains(&class)
            {
                self.dependencies.resolve(self.index, class);
                if self.index.class(class).is_none()
                    && self.index.doc_global_constant(class).is_none()
                    && !self
                        .index
                        .builtin_classes
                        .contains(&class.to_ascii_lowercase())
                {
                    self.report(Finding::UnknownClass(class.into()), span);
                    known = false;
                }
            }
            match ty {
                ValueType::Union(types) | ValueType::Intersection(types) => {
                    pending.extend(types.iter())
                }
                ValueType::Generic(generic) => pending.extend(generic.arguments.iter()),
                ValueType::Collection(collection) => {
                    pending.extend([&collection.key, &collection.value])
                }
                ValueType::Shape(shape) => {
                    pending.extend(shape.fields.iter().map(|field| &field.value))
                }
                ValueType::ClassString(bound) => pending.push(bound),
                ValueType::Callable(signature) => {
                    pending.push(&signature.returns);
                    pending.extend(signature.parameters.iter().map(|parameter| &parameter.ty));
                }
                _ => {}
            }
        }
        known
    }
}

pub(super) fn statement_comments(source: &str) -> std::collections::BTreeMap<usize, Span> {
    use phera_lexer::{Lexer, TokenKind};
    let mut comments = std::collections::BTreeMap::new();
    if !source.contains("@var") && !source.contains("-var") {
        return comments;
    }
    let mut lexer = Lexer::for_tokenizer(source.as_bytes());
    let mut previous = None;
    while let Ok(token) = lexer.next_token() {
        match token.kind {
            TokenKind::Eof => break,
            TokenKind::Whitespace | TokenKind::Comment => {}
            TokenKind::DocComment => previous = Some(token.span),
            _ => {
                if let Some(doc) = previous.take()
                    && crate::phpdoc::text(source, Some(doc))
                        .is_some_and(|text| text.contains("@var") || text.contains("-var"))
                {
                    comments.insert(token.span.start, doc);
                }
            }
        }
    }
    comments
}

impl Analyzer<'_> {
    pub fn statement_var_annotations(
        &mut self,
        statement: &Statement,
        scope: &mut Scope,
    ) -> std::collections::BTreeMap<String, ValueType> {
        let Some(doc) = self.statement_docs.get(&statement.span.start).copied() else {
            return Default::default();
        };
        let assigned = match &statement.kind {
            StatementKind::Expression(expression) => match &expression.kind {
                ExpressionKind::Assign { target, .. } => match &target.kind {
                    ExpressionKind::Variable(Variable::Named(name)) => Some(name.as_str()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        };
        let docs = self.read_documentation(Some(doc));
        let mut after = std::collections::BTreeMap::new();
        for (name, annotation) in docs.variables {
            if let Some(name) = name.as_deref().or(assigned) {
                if assigned == Some(name) {
                    after.insert(name.into(), annotation.ty);
                } else if scope.get(name).definition == Definition::Definite {
                    scope.define(name, annotation.ty);
                }
            }
        }
        after
    }
}
