//! Compile statement constructs into ordered IR and patch their control-flow exits.

use std::sync::Arc;

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
            if key == NameKind::Class
                && super::type_declarations::builtin_type(&alias.to_ascii_lowercase())
            {
                return Err(self.error(
                    format!(
                        "Cannot use {} as {alias} because '{alias}' is a special class name",
                        i.name.text
                    ),
                    self.lines.line(i.name.span.start),
                ));
            }
            let imports = self.imports.get_or_insert_with(Default::default);
            Arc::make_mut(imports).insert((key, alias.to_ascii_lowercase()), i.name.text.clone());
        }

        Ok(())
    }

    /// Evaluate and declare constants in source order.
    pub(crate) fn compile_constants(
        &mut self,
        items: &[ast::ConstantDeclaration],
        metadata: &phera_bytecode::reflection::DeclarationMetadata,
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
            self.constant_metadata
                .entry(name.clone())
                .or_insert_with(|| metadata.clone());
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

    /// The file prelude consists of consecutive declare statements.
    pub(crate) fn compile_file_body(
        &mut self,
        mut statements: &[ast::Statement],
    ) -> Result<(), CompileError> {
        while let Some((first, rest)) = statements.split_first() {
            let ast::StatementKind::Declare { directives, body } = &first.kind else {
                break;
            };
            self.compile_declare(
                directives,
                body,
                DeclarationTiming::Hoisted,
                self.lines.line(first.span.start),
                DeclarePosition::Prelude,
            )?;
            statements = rest;
        }
        self.block(statements, DeclarationTiming::Hoisted)
    }

    /// Apply strict_types from a standalone directive in the file prelude.
    pub(super) fn compile_declare(
        &mut self,
        directives: &[ast::DeclareDirective],
        body: &Option<Vec<ast::Statement>>,
        timing: DeclarationTiming,
        line: usize,
        position: DeclarePosition,
    ) -> Result<(), CompileError> {
        for directive in directives {
            if directive.name.name.eq_ignore_ascii_case("strict_types") {
                let line = self.lines.line(directive.name.span.start);
                let ast::ExpressionKind::Literal(
                    value @ (ast::Literal::Int(_)
                    | ast::Literal::Float(_)
                    | ast::Literal::String(_)),
                ) = &directive.value.kind
                else {
                    return Err(self.error("declare(strict_types) value must be a literal", line));
                };
                if position != DeclarePosition::Prelude {
                    return Err(self.error(
                        "strict_types declaration must be the very first statement in the script",
                        line,
                    ));
                }
                if body.is_some() {
                    return Err(
                        self.error("strict_types declaration must not use block mode", line)
                    );
                }
                self.function.strict_types = match value {
                    ast::Literal::Int(0) => false,
                    ast::Literal::Int(1) => true,
                    _ => {
                        return Err(self.error(
                            "strict_types declaration must have 0 or 1 as its value",
                            line,
                        ));
                    }
                };
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
                if name == "this" {
                    return Err(self.error("Cannot use $this as global variable", line));
                }
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

#[derive(PartialEq, Eq)]
pub(super) enum DeclarePosition {
    Prelude,
    Body,
}
