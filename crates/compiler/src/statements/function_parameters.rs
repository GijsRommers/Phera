//! Function prologues implement defaults and promotion: `function f($n = 1) { return $n; }`.

use phera_ast as ast;

use crate::ir::{Instruction, Place, Target};
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Unsupported declaration metadata must fail before any body is emitted.
    pub(crate) fn validate_parameter_features(
        &self,
        declaration: &super::functions::FunctionSource<'_>,
        line: usize,
    ) -> Result<(), CompileError> {
        for parameter in declaration.parameters {
            if !parameter.hooks.is_empty() {
                return Err(self.error("Property hooks not supported yet", line));
            }
            if parameter.modifiers.promotes_property()
                && !declaration
                    .name
                    .rsplit_once("::")
                    .is_some_and(|(_, method)| method.eq_ignore_ascii_case("__construct"))
            {
                return Err(self.error(
                    "Cannot declare promoted property outside a constructor",
                    self.lines.line(parameter.span.start),
                ));
            }
        }
        Ok(())
    }

    /// Promoted constructor properties are initialized before the user-written body.
    pub(crate) fn compile_promoted_properties(
        &mut self,
        parameters: &[ast::Parameter],
        line: usize,
    ) -> Result<(), CompileError> {
        for parameter in parameters {
            if parameter.modifiers.promotes_property() {
                let scratch = self.allocate_register()?;
                let source = self.local(&parameter.name)?;
                let place = Place {
                    base: scratch,
                    scope: crate::ir::PlaceScope::This,
                    path: vec![crate::ir::PathStep::Property(parameter.name.clone())],
                };
                let instruction = if parameter.by_reference {
                    Instruction::Bind {
                        place,
                        source: Place {
                            base: source,
                            scope: crate::ir::PlaceScope::Local,
                            path: vec![],
                        },
                    }
                } else {
                    Instruction::Write { place, source }
                };
                self.emit(instruction, line);
            }
        }
        Ok(())
    }

    /// Defaults run only for omitted arguments and are checked against their declared type.
    pub(crate) fn compile_parameter_defaults(
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
                            declaration_type: self.declaration_type(ty),
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
