//! Function declarations, parameter prologues, and nested declarations.

use phera_ast as ast;

use crate::ir::{Constant, Instruction, Parameter, Place, Target};
use crate::policies::DeclarationTiming;
use crate::{CompileError, Compiler, type_name};

impl Compiler<'_> {
    /// Build a separate frame, then relocate declarations nested within its body.
    pub(crate) fn compile_function(
        &mut self,
        declaration: &ast::FunctionDeclaration,
        timing: DeclarationTiming,
        line: usize,
    ) -> Result<(), CompileError> {
        let name = if self.namespace.is_empty() {
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
        let mut builder = Compiler::new(self.file, self.lines, &name, &self.namespace);
        builder.function.definition_line = line;
        builder.function.returns_reference = declaration.by_reference;
        builder.imports = self.imports.clone();
        builder.function.strict_types = self.function.strict_types;
        for parameter in &declaration.parameters {
            builder.local(&parameter.name)?;
            builder.function.parameters.push(Parameter {
                name: parameter.name.clone(),
                has_default: parameter.default.is_some(),
                declaration_type: parameter.ty.as_ref().map(type_name),
                by_reference: parameter.by_reference,
                variadic: parameter.variadic,
            });
        }
        builder.compile_parameter_defaults(&declaration.parameters, line)?;
        for parameter in &declaration.parameters {
            if parameter.modifiers.visibility.is_some() {
                let receiver = builder.allocate_register()?;
                builder.emit(
                    Instruction::This {
                        destination: receiver,
                    },
                    line,
                );
                let name =
                    builder.literal(Constant::Bytes(parameter.name.as_bytes().to_vec()), line)?;
                let source = builder.local(&parameter.name)?;
                builder.emit(
                    Instruction::Write {
                        place: Place {
                            base: receiver,
                            scope: crate::ir::PlaceScope::Local,
                            path: vec![crate::ir::PathStep::DynamicProperty(name)],
                        },
                        source,
                    },
                    line,
                );
            }
        }
        builder.function.return_type = declaration.return_type.as_ref().map(type_name);
        builder.block(&declaration.body, DeclarationTiming::AtExecution)?;
        let register = builder.literal(Constant::Null, line)?;
        builder.emit(Instruction::Return(register), line);
        if !builder.classes.is_empty() {
            return Err(self.error("Nested class declarations not supported yet", line));
        }
        let id = self.functions.len();
        let mut function = builder.finish();
        for instruction in function.instructions.iter_mut().chain(
            builder
                .functions
                .iter_mut()
                .flat_map(|declaration| declaration.instructions.iter_mut()),
        ) {
            if let Instruction::Declare(i) = instruction {
                *i += id + 1;
            }
        }
        self.functions.push(function);
        self.functions.extend(builder.functions);
        if timing == DeclarationTiming::Hoisted {
            self.hoisted.push(id)
        } else {
            self.emit(Instruction::Declare(id), line);
        }

        Ok(())
    }
    /// Defaults run only for omitted arguments and are checked against their declared type.
    fn compile_parameter_defaults(
        &mut self,
        parameters: &[ast::Parameter],
        line: usize,
    ) -> Result<(), CompileError> {
        for parameter in parameters {
            if let Some(default) = &parameter.default {
                self.validate_parameter_default(parameter, default, line)?;
                let param = self.local(&parameter.name)?;
                let check = self.emit(
                    Instruction::DefaultCheck {
                        parameter: param,
                        passed: Target(0),
                    },
                    line,
                );
                let src = self.compile_expression(default)?;
                self.emit(
                    Instruction::Write {
                        place: Place {
                            base: param,
                            scope: crate::ir::PlaceScope::Local,
                            path: vec![],
                        },
                        source: src,
                    },
                    line,
                );
                if let Some(ty) = &parameter.ty {
                    self.emit(
                        Instruction::Check {
                            local: param,
                            declaration_type: type_name(ty),
                        },
                        line,
                    );
                }
                self.patch(check, self.position());
            }
        }
        Ok(())
    }
}
