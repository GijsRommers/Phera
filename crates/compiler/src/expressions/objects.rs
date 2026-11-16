//! Object operations preserve callee-before-arguments ordering through the existing call protocol.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::Instruction;
use crate::policies::NameKind;
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Resolve class identifiers at compile time; computed targets remain explicitly unsupported.
    pub(crate) fn compile_object_expression(
        &mut self,
        expression: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        match &expression.kind {
            ast::ExpressionKind::ClassConstant {
                class,
                name: ast::MemberName::Identifier(name),
            } if name.name.eq_ignore_ascii_case("class")
                && matches!(class.kind, ast::ExpressionKind::Name(_)) =>
            {
                let class = self.object_class_name(class, line)?;
                let destination = self.allocate_register()?;
                self.emit(Instruction::ClassName { destination, class }, line);
                Ok(destination)
            }
            ast::ExpressionKind::ClassConstant {
                class,
                name: ast::MemberName::Identifier(name),
            } => {
                let class = self.compile_class_target(class, line)?;
                let destination = self.allocate_register()?;
                self.emit(
                    Instruction::ClassConstant {
                        destination,
                        class,
                        name: name.name.clone(),
                    },
                    line,
                );
                Ok(destination)
            }
            ast::ExpressionKind::New {
                class: ast::NewTarget::Class(class),
                arguments,
            } => {
                let class = self.object_class_name(class, line)?;
                self.emit(Instruction::New { class }, line);
                self.compile_arguments(arguments, line)
            }
            ast::ExpressionKind::MethodCall {
                target,
                name: ast::MemberName::Identifier(name),
                arguments,
                nullsafe: false,
            } => {
                let receiver = self.compile_expression(target)?;
                self.emit(
                    Instruction::MethodCall {
                        receiver,
                        method: name.name.clone(),
                    },
                    line,
                );
                self.compile_arguments(arguments, line)
            }
            ast::ExpressionKind::Clone { object, with: None } => {
                let source = self.compile_expression(object)?;
                let destination = self.allocate_register()?;
                self.emit(
                    Instruction::Clone {
                        destination,
                        source,
                    },
                    line,
                );
                Ok(destination)
            }
            ast::ExpressionKind::InstanceOf { value, class } => {
                let class = self.object_class_name(class, line)?;
                let source = self.compile_expression(value)?;
                let destination = self.allocate_register()?;
                self.emit(
                    Instruction::InstanceOf {
                        destination,
                        source,
                        class,
                    },
                    line,
                );
                Ok(destination)
            }
            _ => Err(self.error("Object expression not supported yet", line)),
        }
    }

    /// Class targets may be literal identifiers, objects, or evaluated class-name strings.
    pub(crate) fn compile_class_target(
        &mut self,
        expression: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        if matches!(expression.kind, ast::ExpressionKind::Name(_)) {
            let class = self.object_class_name(expression, line)?;
            let destination = self.allocate_register()?;
            self.emit(Instruction::ClassName { destination, class }, line);
            Ok(destination)
        } else {
            self.compile_expression(expression)
        }
    }

    /// Class names never use the function/constant global namespace fallback.
    pub(crate) fn object_class_name(
        &self,
        expression: &ast::Expression,
        line: usize,
    ) -> Result<String, CompileError> {
        let ast::ExpressionKind::Name(name) = &expression.kind else {
            return Err(self.error("Dynamic class names not supported yet", line));
        };
        if matches!(
            name.text.to_ascii_lowercase().as_str(),
            "self" | "parent" | "static"
        ) {
            return Ok(name.text.to_ascii_lowercase());
        }
        Ok(self.resolve(name, NameKind::Class).0)
    }
}
