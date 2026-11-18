//! `#[Route(new Options())]` stores initializer code until reflection reads the argument.

use std::collections::HashSet;

use phera_ast::{Attribute, Expression, ExpressionKind, MemberName};
use phera_bytecode::FunctionId;
use phera_bytecode::reflection::{AttributeArgument, DeclarationMetadata, ParameterMetadata};
use phera_source::Span;

use crate::{CompileError, Compiler, ir::Instruction};

impl Compiler<'_> {
    pub(super) fn declaration_metadata(
        &mut self,
        attributes: &[Attribute],
        span: Span,
        doc_comment: Option<Span>,
        scope: Option<&str>,
    ) -> Result<DeclarationMetadata, CompileError> {
        Ok(DeclarationMetadata {
            start_line: self.lines.line(span.start) as u32,
            end_line: self.lines.line(span.end.saturating_sub(1)) as u32,
            doc_comment: doc_comment.and_then(|comment| {
                self.source
                    .get(comment.start..comment.end)
                    .map(<[u8]>::to_vec)
            }),
            attributes: self.compile_attributes(attributes, scope)?,
        })
    }

    pub(super) fn compile_attributes(
        &mut self,
        attributes: &[Attribute],
        scope: Option<&str>,
    ) -> Result<Vec<phera_bytecode::reflection::Attribute>, CompileError> {
        attributes
            .iter()
            .map(|attribute| {
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
                    let initializer =
                        self.compile_reflection_initializer(&argument.value, scope, line)?;
                    arguments.push(AttributeArgument { name, initializer });
                }
                let name = self
                    .resolve(&attribute.name, crate::policies::NameKind::Class)
                    .0;
                Ok(phera_bytecode::reflection::Attribute { name, arguments })
            })
            .collect()
    }

    pub(super) fn reflected_parameter(
        &mut self,
        parameter: &phera_ast::Parameter,
        line: usize,
    ) -> Result<ParameterMetadata, CompileError> {
        let scope = self.class_scope.clone();
        let default = parameter
            .default
            .as_ref()
            .map(|expression| {
                self.compile_reflection_initializer(expression, scope.as_deref(), line)
            })
            .transpose()?;
        Ok(ParameterMetadata {
            attributes: self.compile_attributes(&parameter.attributes, scope.as_deref())?,
            default,
            default_constant: parameter
                .default
                .as_ref()
                .and_then(|expr| self.default_constant(expr)),
            promoted: parameter.modifiers.visibility.is_some(),
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

    fn compile_reflection_initializer(
        &mut self,
        expression: &Expression,
        scope: Option<&str>,
        line: usize,
    ) -> Result<FunctionId, CompileError> {
        if let Some(message) = crate::constant_callables::callable_constant_error(expression) {
            return Err(self.error(message, line));
        }
        if let Some(message) = crate::parameter_defaults::initializer_error(expression, true) {
            return Err(self.error(message, line));
        }
        let mut builder = Compiler::new(
            self.file,
            self.lines,
            &self.function.name,
            &self.namespace,
            self.compile_diagnostics,
        );
        builder.source = self.source;
        builder.imports = self.imports.clone();
        builder.class_scope = scope.map(str::to_owned);
        builder.function.strict_types = self.function.strict_types;
        let result = builder.compile_expression(expression)?;
        builder.function.name = "{reflection initializer}".into();
        builder.emit(Instruction::Return(result), line);
        let index = self.functions.len();
        self.append_function(builder, crate::policies::DeclarationTiming::Hidden, line);
        Ok(FunctionId(index as u32))
    }
}
