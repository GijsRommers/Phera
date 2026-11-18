//! `set => trim($value)` writes to the backing slot, and `get => expr`
//! returns through the ordinary method return checks.

use phera_ast as ast;

use crate::ir::{Constant, Instruction, PathStep, Place, PlaceScope};
use crate::policies::DeclarationTiming;
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    pub(super) fn compile_hook_body(
        &mut self,
        body: &ast::HookBody,
        setter: bool,
        end_line: usize,
    ) -> Result<(), CompileError> {
        match body {
            ast::HookBody::Expression(expression) if setter => {
                let line = self.lines.line(expression.span.start);
                let name = self.hooked_property.clone().unwrap_or_default();
                self.uses_hook_backing = true;
                let base = self.allocate_register()?;
                let source = self.compile_expression(expression)?;
                self.emit(
                    Instruction::Write {
                        place: Place {
                            base,
                            scope: PlaceScope::This,
                            path: vec![PathStep::Property(name)],
                        },
                        source,
                    },
                    line,
                );
            }
            ast::HookBody::Expression(expression) => {
                self.compile_return(Some(expression), self.lines.line(expression.span.start))?;
            }
            ast::HookBody::Block(block) => self.block(block, DeclarationTiming::AtExecution)?,
            ast::HookBody::None => {}
        }
        if !setter && self.function.return_type.is_some() {
            self.emit(Instruction::ReturnMissing, end_line);
        } else {
            let register = self.literal(Constant::Null, end_line)?;
            let instruction = if self.function.returns_reference {
                Instruction::ReturnReference(register)
            } else {
                Instruction::Return(register)
            };
            self.emit(instruction, end_line);
        }
        Ok(())
    }

    /// Only a literal access on `$this` in the hook body declares backing storage.
    pub(crate) fn note_hook_backing(&mut self, expression: &ast::Expression) {
        let Some(property) = &self.hooked_property else {
            return;
        };
        let ast::ExpressionKind::Property { target, name, .. } = &expression.kind else {
            return;
        };
        if matches!(&target.kind, ast::ExpressionKind::Variable(ast::Variable::Named(name)) if name == "this")
            && match name {
                ast::MemberName::Identifier(name) => &name.name == property,
                ast::MemberName::Expression(expression) => matches!(
                    &expression.kind, ast::ExpressionKind::Literal(ast::Literal::String(name))
                        if name.as_slice() == property.as_bytes()
                ),
            }
        {
            self.uses_hook_backing = true;
        }
    }
}
