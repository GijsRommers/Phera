//! Function prologues implement defaults and promotion: `function f($n = 1) { return $n; }`.

use phera_ast as ast;

use crate::ir::{Instruction, Place, Target};
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Unsupported declaration metadata must fail before any body is emitted.
    pub(crate) fn validate_function_attributes(
        &self,
        declaration: &ast::FunctionDeclaration,
        line: usize,
    ) -> Result<(), CompileError> {
        if !declaration.attributes.is_empty() {
            return Err(self.error("Attributes not supported yet", line));
        }
        for parameter in &declaration.parameters {
            if !parameter.attributes.is_empty() {
                return Err(self.error("Attributes not supported yet", line));
            }
            if !parameter.hooks.is_empty() {
                return Err(self.error("Property hooks not supported yet", line));
            }
            if parameter.modifiers.set_visibility.is_some() {
                return Err(self.error("Asymmetric visibility not supported yet", line));
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
            if parameter.modifiers.visibility.is_some() {
                // Compiled exactly like a hand-written `$this->name = $name;`,
                // so promotion gets the same cached property write.
                let scratch = self.allocate_register()?;
                let source = self.local(&parameter.name)?;
                self.emit(
                    Instruction::Write {
                        place: Place {
                            base: scratch,
                            scope: crate::ir::PlaceScope::This,
                            path: vec![crate::ir::PathStep::Property(parameter.name.clone())],
                        },
                        source,
                    },
                    line,
                );
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
