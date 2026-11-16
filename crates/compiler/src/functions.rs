//! Function declarations, parameter prologues, and nested declarations.
use phera_ast as ast;

use crate::ir::{Constant, Instruction, Parameter, Place, Target};
use crate::{CompileError, Compiler, type_name};
impl Compiler<'_> {
    pub(super) fn compile_function(
        &mut self,
        declaration: &ast::FunctionDeclaration,
        hoist: bool,
        line: usize,
    ) -> Result<(), CompileError> {
        let name = if self.namespace.is_empty() {
            declaration.name.name.clone()
        } else {
            format!("{}\\{}", self.namespace, declaration.name.name)
        };
        if hoist
            && self
                .hoisted
                .iter()
                .filter_map(|i| self.functions.get(*i))
                .any(|declaration| declaration.name.eq_ignore_ascii_case(&name))
        {
            return Err(self.error(format!("Cannot redeclare function {name}()"), line));
        }
        let mut sub = Compiler::new(self.file, self.lines, &name, &self.namespace);
        sub.fun.returns_reference = declaration.by_reference;
        sub.imports = self.imports.clone();
        sub.fun.strict_types = self.fun.strict_types;
        for parameter in &declaration.parameters {
            sub.local(&parameter.name)?;
            sub.fun.parameters.push(Parameter {
                name: parameter.name.clone(),
                has_default: parameter.default.is_some(),
                ty: parameter.ty.as_ref().map(type_name),
                by_reference: parameter.by_reference,
                variadic: parameter.variadic,
            });
        }
        for parameter in &declaration.parameters {
            if let Some(default) = &parameter.default {
                let param = sub.local(&parameter.name)?;
                let check = sub.emit(
                    Instruction::DefaultCheck {
                        param,
                        passed: Target(0),
                    },
                    line,
                );
                let src = sub.expr(default)?;
                sub.emit(
                    Instruction::Write {
                        place: Place {
                            base: param,
                            dynamic: false,
                            path: vec![],
                        },
                        src,
                    },
                    line,
                );
                if let Some(ty) = &parameter.ty {
                    sub.emit(
                        Instruction::Check {
                            local: param,
                            ty: type_name(ty),
                        },
                        line,
                    );
                }
                sub.patch(check, sub.pos());
            }
        }
        sub.fun.return_type = declaration.return_type.as_ref().map(type_name);
        sub.block(&declaration.body, false)?;
        let register = sub.literal(Constant::Null, line)?;
        sub.emit(Instruction::Return(register), line);
        let id = self.functions.len();
        let mut function = sub.finish();
        for instruction in function.instructions.iter_mut().chain(
            sub.functions
                .iter_mut()
                .flat_map(|declaration| declaration.instructions.iter_mut()),
        ) {
            if let Instruction::Declare(i) = instruction {
                *i += id + 1;
            }
        }
        self.functions.push(function);
        self.functions.extend(sub.functions);
        if hoist {
            self.hoisted.push(id)
        } else {
            self.emit(Instruction::Declare(id), line);
        }

        Ok(())
    }
}
