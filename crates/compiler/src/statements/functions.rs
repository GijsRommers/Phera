//! Function declarations, parameter prologues, and nested declarations.

use super::builtin_attributes::AttributePlacement;
use phera_ast as ast;
use phera_source::attributes::AttributeTarget;

use crate::ir::{Constant, Instruction, Parameter};
use crate::policies::DeclarationTiming;
use crate::{CompileError, Compiler};

/// Borrow AST nodes while lowering so long expression trees are never cloned.
pub(crate) struct FunctionSource<'a> {
    pub(crate) span: phera_source::Span,
    pub(crate) doc_comment: Option<phera_source::Span>,
    pub(crate) attributes: &'a [ast::Attribute],
    pub(crate) name: &'a str,
    pub(crate) by_reference: bool,
    pub(crate) parameters: &'a [ast::Parameter],
    pub(crate) return_type: Option<&'a ast::Type>,
    pub(crate) body: FunctionBody<'a>,
}

pub(crate) enum FunctionBody<'a> {
    Statements(&'a [ast::Statement]),
    Arrow(&'a ast::Expression),
}

impl<'ast> Compiler<'ast> {
    /// Build a separate frame, then relocate declarations nested within its body.
    pub(crate) fn compile_function(
        &mut self,
        declaration: &ast::FunctionDeclaration,
        timing: DeclarationTiming,
        line: usize,
    ) -> Result<(), CompileError> {
        self.compile_captured_function(
            &FunctionSource {
                span: declaration.span,
                doc_comment: declaration.doc_comment,
                attributes: &declaration.attributes,
                name: &declaration.name.name,
                by_reference: declaration.by_reference,
                parameters: &declaration.parameters,
                return_type: declaration.return_type.as_ref(),
                body: FunctionBody::Statements(&declaration.body),
            },
            timing,
            line,
            &[],
        )
    }

    /// Captures must be allocated before default expressions introduce other locals.
    pub(crate) fn compile_captured_function(
        &mut self,
        declaration: &FunctionSource<'_>,
        timing: DeclarationTiming,
        line: usize,
        captures: &[String],
    ) -> Result<(), CompileError> {
        self.validate_parameter_features(declaration)?;
        let name = if self.namespace.is_empty()
            || declaration.name.contains("::")
            || declaration.name.starts_with("{closure:")
        {
            declaration.name.to_owned()
        } else {
            format!("{}\\{}", self.namespace, declaration.name)
        };
        if timing == DeclarationTiming::Hoisted
            && self
                .hoisted
                .iter()
                .filter_map(|i| self.functions.get(*i))
                .any(|declaration| declaration.name.eq_ignore_ascii_case(&name))
        {
            return Err(self.error(format!("Cannot redeclare function {name}()"), line));
        }
        let mut builder = self.function_builder(declaration, &name, captures, line)?;
        if builder.has_builtin(declaration.attributes, "Deprecated")
            || builder.has_builtin(declaration.attributes, "NoDiscard")
        {
            builder.emit(Instruction::AttributeDiagnostics, line);
        }
        builder.compile_parameter_defaults(declaration.parameters, line)?;
        let scope = builder.class_scope.clone();
        builder.function.metadata.declaration = builder.declaration_metadata(
            declaration.attributes,
            declaration.span,
            declaration.doc_comment,
            scope.as_deref(),
            AttributePlacement::Declaration(
                if declaration.name.contains("::") && !declaration.name.starts_with("{closure:") {
                    AttributeTarget::Method
                } else {
                    AttributeTarget::Function
                },
            ),
        )?;
        builder.validate_nodiscard(declaration, line)?;
        if declaration.parameters.iter().any(|parameter| {
            parameter.default.is_some()
                || !parameter.attributes.is_empty()
                || parameter.is_promoted()
        }) {
            builder.function.metadata.parameters = declaration
                .parameters
                .iter()
                .map(|parameter| builder.reflected_parameter(parameter))
                .collect::<Result<_, _>>()?;
        }
        builder.compile_promoted_properties(declaration.parameters, line)?;
        builder.function.return_type = declaration
            .return_type
            .map(|declared_type| self.declaration_type(declared_type));
        match declaration.body {
            FunctionBody::Statements(body) => {
                builder.block(body, DeclarationTiming::AtExecution)?
            }
            FunctionBody::Arrow(body) => builder.compile_return(Some(body), line)?,
        }
        let implicit_return = if declaration
            .name
            .rsplit_once("::")
            .is_some_and(|(_, method)| method.eq_ignore_ascii_case("__toString"))
        {
            Instruction::ReturnMissing
        } else {
            let register = builder.literal(Constant::Null, line)?;
            if declaration.by_reference {
                Instruction::ReturnReference(register)
            } else {
                Instruction::Return(register)
            }
        };
        builder.emit(implicit_return, line);
        self.append_function(builder, timing, line)
    }

    /// Nested declarations use indices relative to their builder until attached to the unit.
    pub(super) fn append_function(
        &mut self,
        mut builder: Compiler<'_>,
        timing: DeclarationTiming,
        line: usize,
    ) -> Result<(), CompileError> {
        let id = self.append_function_body(&mut builder)?;
        if timing == DeclarationTiming::Hoisted {
            self.hoisted.push(id)
        } else if timing == DeclarationTiming::AtExecution {
            self.emit(Instruction::Declare(id), line);
        }
        Ok(())
    }

    /// Finalize the lexical frame and relocate its code and metadata.
    pub(crate) fn append_function_body(
        &mut self,
        builder: &mut Compiler<'_>,
    ) -> Result<usize, CompileError> {
        let id = self.functions.len();
        let class_offset = self.classes.len();
        let mut function = builder.finish()?;
        for instruction in function.instructions.iter_mut().chain(
            builder
                .functions
                .iter_mut()
                .flat_map(|declaration| declaration.instructions.iter_mut()),
        ) {
            match instruction {
                Instruction::Declare(index)
                | Instruction::Closure {
                    function: index, ..
                } => {
                    *index += id + 1;
                }
                Instruction::DeclareClass(index) => *index += class_offset,
                _ => {}
            }
        }
        for class in &mut builder.classes {
            for method in &mut class.methods {
                method.function += id + 1;
            }
            for constant in &mut class.constants {
                constant.initializer += id + 1;
            }
            for property in &mut class.properties {
                if let Some(hooks) = &mut property.hooks {
                    for hook in hooks.get.iter_mut().chain(hooks.set.iter_mut()) {
                        hook.function += id + 1;
                    }
                }
                if let Some(default) = &mut property.default {
                    *default += id + 1;
                }
            }
            for (_, backing, _) in &mut class.enum_cases {
                if let Some(initializer) = backing {
                    *initializer += id + 1;
                }
            }
        }
        self.functions.push(function);
        self.functions.append(&mut builder.functions);
        self.classes.append(&mut builder.classes);
        self.declaration_warnings
            .append(&mut builder.declaration_warnings);
        Ok(id)
    }

    /// Independent builders keep captured locals ahead of ordinary body temporaries.
    pub(super) fn function_builder(
        &self,
        declaration: &FunctionSource<'_>,
        name: &str,
        captures: &[String],
        line: usize,
    ) -> Result<Compiler<'ast>, CompileError> {
        let mut builder = Compiler::new(
            self.file,
            self.source,
            self.lines,
            name,
            &self.namespace,
            self.compile_diagnostics,
            self.reflection,
        );
        builder.anonymous_classes = self.anonymous_classes.clone();
        if name.starts_with("{closure:") || name.contains("::") {
            builder.trait_scope = self.trait_scope.clone();
        }
        builder.class_scope = if !name.starts_with("{closure:") {
            name.split_once("::")
                .map(|(class, _)| class.to_owned())
                .or_else(|| self.class_scope.clone())
        } else {
            self.class_scope.clone()
        };
        builder.function.definition_line = line;
        builder.function.returns_reference = declaration.by_reference;
        builder.imports = self.imports.clone();
        builder.function.strict_types = self.function.strict_types;
        for parameter in declaration.parameters {
            builder.local(&parameter.name)?;
            builder.function.parameters.push(Parameter {
                default_display: parameter
                    .default
                    .as_ref()
                    .map(crate::helpers::default_display),
                name: parameter.name.clone(),
                has_default: parameter.default.is_some(),
                declaration_type: parameter
                    .ty
                    .as_ref()
                    .map(|declared_type| self.declaration_type(declared_type)),
                by_reference: parameter.by_reference,
                variadic: parameter.variadic,
            });
        }
        for name in captures {
            builder.local(name)?;
        }
        builder.function.captures = captures.len();
        Ok(builder)
    }
}
