//! Function declarations, parameter prologues, and nested declarations.

use phera_ast as ast;

use crate::ir::{Constant, Instruction, Parameter};
use crate::policies::DeclarationTiming;
use crate::{CompileError, Compiler};

impl<'ast> Compiler<'ast> {
    /// Build a separate frame, then relocate declarations nested within its body.
    pub(crate) fn compile_function(
        &mut self,
        declaration: &ast::FunctionDeclaration,
        timing: DeclarationTiming,
        line: usize,
    ) -> Result<(), CompileError> {
        self.compile_captured_function(declaration, timing, line, &[])
    }

    /// Captures must be allocated before default expressions introduce other locals.
    pub(crate) fn compile_captured_function(
        &mut self,
        declaration: &ast::FunctionDeclaration,
        timing: DeclarationTiming,
        line: usize,
        captures: &[String],
    ) -> Result<(), CompileError> {
        self.validate_function_attributes(declaration, line)?;
        let name = if self.namespace.is_empty()
            || declaration.name.name.contains("::")
            || declaration.name.name.starts_with("{closure:")
        {
            declaration.name.name.clone()
        } else {
            format!("{}\\{}", self.namespace, declaration.name.name)
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
        builder.compile_parameter_defaults(&declaration.parameters, line)?;
        builder.compile_promoted_properties(&declaration.parameters, line)?;
        builder.function.return_type = declaration
            .return_type
            .as_ref()
            .map(|declared_type| self.declaration_type(declared_type));
        builder.block(&declaration.body, DeclarationTiming::AtExecution)?;
        let implicit_return = if declaration
            .name
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
        self.append_function(builder, timing, line);
        Ok(())
    }

    /// Nested declarations use indices relative to their builder until attached to the unit.
    fn append_function(
        &mut self,
        mut builder: Compiler<'_>,
        timing: DeclarationTiming,
        line: usize,
    ) {
        let id = self.append_function_body(&mut builder);
        if timing == DeclarationTiming::Hoisted {
            self.hoisted.push(id)
        } else {
            self.emit(Instruction::Declare(id), line);
        }
    }

    /// Relocate code and metadata compiled in a separate lexical frame.
    pub(crate) fn append_function_body(&mut self, builder: &mut Compiler<'_>) -> usize {
        let id = self.functions.len();
        let class_offset = self.classes.len();
        let mut function = builder.finish();
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
                if let Some(default) = &mut property.default {
                    *default += id + 1;
                }
            }
            for (_, backing) in &mut class.enum_cases {
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
        id
    }

    /// Independent builders keep captured locals ahead of ordinary body temporaries.
    fn function_builder(
        &self,
        declaration: &ast::FunctionDeclaration,
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
        for parameter in &declaration.parameters {
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
