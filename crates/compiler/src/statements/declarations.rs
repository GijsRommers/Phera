//! Compile statement constructs into ordered IR and patch their control-flow exits.

use phera_ast as ast;

use crate::ir::{Constant, Instruction, Target};
use crate::policies::{DeclarationTiming, NameKind};
use crate::{CompileError, Compiler, constant};

impl Compiler<'_> {
    /// Record aliases for later namespace resolution.
    pub(crate) fn compile_use(&mut self, items: &[ast::UseItem]) -> Result<(), CompileError> {
        for i in items {
            let alias = i
                .alias
                .as_ref()
                .map(|x| x.name.clone())
                .unwrap_or_else(|| i.name.text.rsplit('\\').next().unwrap_or("").into());
            let key = match i.kind {
                ast::UseKind::Class => NameKind::Class,
                ast::UseKind::Function => NameKind::Function,
                ast::UseKind::Constant => NameKind::Constant,
            };
            self.imports
                .insert((key, alias.to_ascii_lowercase()), i.name.text.clone());
        }

        Ok(())
    }

    /// Evaluate and declare constants in source order.
    pub(crate) fn compile_constants(
        &mut self,
        items: &[ast::ConstantDeclaration],
        line: usize,
    ) -> Result<(), CompileError> {
        for i in items {
            if let Some(message) = crate::constant_callables::callable_constant_error(&i.value) {
                return Err(self.error(message, line));
            }
            let register = self.compile_expression(&i.value)?;
            let name = if self.namespace.is_empty() {
                i.name.name.clone()
            } else {
                format!("{}\\{}", self.namespace, i.name.name)
            };
            self.emit(
                Instruction::Define {
                    name,
                    source: register,
                },
                line,
            );
        }

        Ok(())
    }

    /// Apply strict_types to the function containing the declaration.
    pub(crate) fn compile_declare(
        &mut self,
        directives: &[ast::DeclareDirective],
        body: &Option<Vec<ast::Statement>>,
        timing: DeclarationTiming,
        line: usize,
    ) -> Result<(), CompileError> {
        for directive in directives {
            if directive.name.name == "strict_types" {
                self.function.strict_types =
                    matches!(constant(&directive.value), Some(Constant::Int(1)))
            } else if directive.name.name != "ticks"
                || !matches!(constant(&directive.value), Some(Constant::Int(0)))
            {
                return Err(self.error(
                    format!("declare({}) not supported yet", directive.name.name),
                    line,
                ));
            }
        }
        if let Some(body) = body {
            self.block(body, timing)?
        }

        Ok(())
    }

    /// Bind local variables to the request global scope.
    pub(crate) fn compile_globals(
        &mut self,
        variables: &[ast::Variable],
        line: usize,
    ) -> Result<(), CompileError> {
        for value in variables {
            if let ast::Variable::Named(name) = value {
                let local = self.local(name)?;
                self.emit(
                    Instruction::Global {
                        local,
                        name: name.clone(),
                    },
                    line,
                );
            } else {
                if let ast::Variable::Dynamic(expression) = value {
                    self.function.dynamic_scope = true;
                    let name = self.compile_expression(expression)?;
                    self.emit(Instruction::DynamicGlobal { name }, line);
                }
            }
        }

        Ok(())
    }

    /// Skip initialization when the static variable already has a value.
    pub(crate) fn compile_statics(
        &mut self,
        variables: &[ast::StaticVariable],
        line: usize,
    ) -> Result<(), CompileError> {
        for variable in variables {
            let local = self.local(&variable.name)?;
            let check = self.emit(
                Instruction::StaticCheck {
                    local,
                    name: variable.name.clone(),
                    initialized: Target(0),
                },
                line,
            );
            let src = if let Some(default) = &variable.default {
                self.compile_expression(default)?
            } else {
                self.literal(Constant::Null, line)?
            };
            self.emit(
                Instruction::StaticInit {
                    local,
                    name: variable.name.clone(),
                    source: src,
                },
                line,
            );
            self.patch(check, self.position());
        }

        Ok(())
    }
}
