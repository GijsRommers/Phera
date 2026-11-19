//! `#[Route(new Options())]` stores initializer code until reflection reads the argument.

use std::collections::HashSet;

use phera_ast::{Attribute, Expression, ExpressionKind, Literal, MemberName};
use phera_bytecode::reflection::{
    AttributeArgument, DeclarationMetadata, Initializer, InitializerId, InitializerSource,
    ParameterMetadata,
};
use phera_source::{Span, attributes::AttributeTarget};

use super::builtin_attributes::AttributePlacement;

use crate::{CompileError, Compiler};

impl Compiler<'_> {
    pub(super) fn declaration_metadata(
        &self,
        attributes: &[Attribute],
        span: Span,
        doc_comment: Option<Span>,
        scope: Option<&str>,
        placement: AttributePlacement,
    ) -> Result<DeclarationMetadata, CompileError> {
        if doc_comment.is_some() {
            self.reflection.has_doc_comments.set(true);
        }
        let attributes = if attributes.is_empty() {
            Vec::new()
        } else {
            self.compile_attributes(attributes, scope, placement)?
        };
        Ok(DeclarationMetadata {
            span,
            doc_comment: doc_comment.unwrap_or_default(),
            attributes,
        })
    }

    pub(super) fn compile_attributes(
        &self,
        attributes: &[Attribute],
        scope: Option<&str>,
        placement: AttributePlacement,
    ) -> Result<Vec<phera_bytecode::reflection::Attribute>, CompileError> {
        if attributes.is_empty() {
            return Ok(Vec::new());
        }
        let mut compiled = Vec::with_capacity(attributes.len());
        let placement = match placement {
            AttributePlacement::Promoted(target)
                if self.has_builtin(attributes, "DelayedTargetValidation") =>
            {
                AttributePlacement::Declaration(target)
            }
            placement => placement,
        };
        for attribute in attributes {
            if self.attribute_includes_target(attribute, placement) {
                let line = self.lines.line(attribute.span.start);
                if attribute.first_class_callable {
                    return Err(self.error("Cannot create Closure as attribute argument", line));
                }
                let mut arguments = Vec::with_capacity(attribute.arguments.len());
                let mut named = HashSet::new();
                for argument in &attribute.arguments {
                    let line = self.lines.line(argument.span.start);
                    if argument.spread {
                        return Err(
                            self.error("Cannot use unpacking in attribute argument list", line)
                        );
                    }
                    let name = argument.name.as_ref().map(|name| name.name.clone());
                    match &name {
                        Some(name) if !named.insert(name.clone()) => {
                            return Err(
                                self.error(format!("Duplicate named parameter ${name}"), line)
                            );
                        }
                        None if !named.is_empty() => {
                            return Err(self.error(
                                "Cannot use positional argument after named argument",
                                line,
                            ));
                        }
                        _ => {}
                    }
                    if let Some(message) = crate::constant_callables::callable_constant_error(
                        &argument.value,
                    )
                    .or_else(|| crate::parameter_defaults::initializer_error(&argument.value, true))
                    {
                        return Err(self.error(message, line));
                    }
                    let initializer = self.remember_initializer(&argument.value, scope);
                    arguments.push(AttributeArgument { name, initializer });
                }
                let name = self
                    .resolve(&attribute.name, crate::policies::NameKind::Class)
                    .0;
                if name.eq_ignore_ascii_case("Override") || name.eq_ignore_ascii_case("Deprecated")
                {
                    self.reflection.has_link_attributes.set(true);
                }
                compiled.push(phera_bytecode::reflection::Attribute {
                    name,
                    line: self.lines.line(attribute.span.start),
                    strict_types: self.function.strict_types,
                    validation_error: None,
                    arguments,
                });
            }
        }
        self.validate_attribute_list(attributes, placement)?;
        Ok(compiled)
    }

    pub(super) fn reflected_parameter(
        &self,
        parameter: &phera_ast::Parameter,
    ) -> Result<ParameterMetadata, CompileError> {
        let scope = self.class_scope.as_deref();
        let default = parameter
            .default
            .as_ref()
            .map(|expression| self.remember_initializer(expression, scope));
        Ok(ParameterMetadata {
            attributes: self.compile_attributes(
                &parameter.attributes,
                scope,
                if parameter.is_promoted() {
                    AttributePlacement::Promoted(AttributeTarget::Parameter)
                } else {
                    AttributePlacement::Declaration(AttributeTarget::Parameter)
                },
            )?,
            default,
            default_constant: parameter
                .default
                .as_ref()
                .and_then(|expr| self.default_constant(expr)),
            promoted: parameter.is_promoted(),
        })
    }

    fn default_constant(&self, expression: &Expression) -> Option<String> {
        match &expression.kind {
            ExpressionKind::Name(name) => {
                Some(self.resolve(name, crate::policies::NameKind::Constant).0)
            }
            ExpressionKind::ClassConstant {
                class,
                name: MemberName::Identifier(name),
            } if !name.name.eq_ignore_ascii_case("class") => {
                let class = self
                    .object_class_name(class, self.lines.line(class.span.start))
                    .ok()?;
                Some(format!("{class}::{}", name.name))
            }
            _ => None,
        }
    }

    fn remember_initializer(&self, expression: &Expression, scope: Option<&str>) -> Initializer {
        match &expression.kind {
            ExpressionKind::Literal(value) => {
                return match value {
                    Literal::Null => Initializer::Null,
                    Literal::Bool(value) => Initializer::Bool(*value),
                    Literal::Int(value) => Initializer::Int(*value),
                    Literal::Float(value) => Initializer::Float(value.to_bits()),
                    Literal::String(value) => Initializer::String(value.clone()),
                };
            }
            ExpressionKind::Array(items) if items.is_empty() => return Initializer::EmptyArray,
            _ => {}
        }
        let mut initializers = self.reflection.initializers.borrow_mut();
        let index = initializers.len();
        initializers.push(InitializerSource {
            span: expression.span,
            first_line: self.lines.line(expression.span.start),
            function: self.function.name.clone(),
            namespace: self.namespace.clone(),
            scope: scope.map(str::to_owned),
            imports: self.imports.clone(),
            strict_types: self.function.strict_types,
        });
        Initializer::Expression(InitializerId(index))
    }
}
