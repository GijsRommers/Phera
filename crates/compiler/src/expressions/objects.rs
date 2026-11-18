//! Object operations preserve callee-before-arguments ordering through the existing call protocol.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::Instruction;
use crate::policies::{DeclarationTiming, NameKind};
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
                class: ast::NewTarget::Anonymous(declaration),
                arguments,
            } => {
                if matches!(arguments, ast::Arguments::FirstClassCallable) {
                    return Err(self.error("Cannot create Closure for new expression", line));
                }
                let class_line = self.lines.line(declaration.span.start);
                let class =
                    self.compile_class(declaration, DeclarationTiming::AtExecution, class_line)?;
                self.emit(Instruction::New { class }, line);
                self.compile_arguments(arguments, line)
            }
            ast::ExpressionKind::ClassConstant {
                class,
                name: ast::MemberName::Expression(name),
            } => {
                let class = self.compile_class_target(class, line)?;
                let name = self.compile_expression(name)?;
                let destination = self.allocate_register()?;
                self.emit(
                    Instruction::DynamicClassConstant {
                        destination,
                        class,
                        name,
                    },
                    line,
                );
                Ok(destination)
            }
            ast::ExpressionKind::New {
                class: ast::NewTarget::Class(class),
                arguments,
            } => {
                if matches!(arguments, ast::Arguments::FirstClassCallable) {
                    return Err(self.error("Cannot create Closure for new expression", line));
                }
                if matches!(class.kind, ast::ExpressionKind::Name(_)) {
                    let class = self.object_class_name(class, line)?;
                    self.emit(Instruction::New { class }, line);
                } else {
                    let class = self.compile_class_target(class, line)?;
                    self.emit(Instruction::NewDynamic { class }, line);
                }
                self.compile_arguments(arguments, line)
            }
            ast::ExpressionKind::MethodCall {
                target,
                name,
                arguments,
                nullsafe: false,
            } => {
                let receiver = self.compile_expression(target)?;
                match name {
                    ast::MemberName::Identifier(name) => self.emit(
                        Instruction::MethodCall {
                            receiver,
                            method: name.name.clone(),
                        },
                        line,
                    ),
                    ast::MemberName::Expression(expression) => {
                        let method = self.compile_expression(expression)?;
                        self.emit(Instruction::DynamicMethodCall { receiver, method }, line)
                    }
                };
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
            ast::ExpressionKind::StaticCall {
                class,
                name,
                arguments,
            } => {
                let binding = if matches!(&class.kind, ast::ExpressionKind::Name(name)
                    if matches!(name.text.to_ascii_lowercase().as_str(), "self" | "static" | "parent"))
                {
                    phera_bytecode::ClassBinding::Forward
                } else {
                    phera_bytecode::ClassBinding::Explicit
                };
                if matches!(class.kind, ast::ExpressionKind::Name(_))
                    && let ast::MemberName::Identifier(name) = name
                {
                    let class = self.object_class_name(class, line)?;
                    self.emit(
                        Instruction::KnownStaticCall {
                            class,
                            method: name.name.clone(),
                            binding,
                        },
                        line,
                    );
                    return self.compile_arguments(arguments, line);
                }
                let class = self.compile_class_target(class, line)?;
                let method = match name {
                    ast::MemberName::Identifier(name) => self.literal(
                        crate::ir::Constant::Bytes(name.name.as_bytes().to_vec()),
                        line,
                    )?,
                    ast::MemberName::Expression(expression) => {
                        self.compile_expression(expression)?
                    }
                };
                self.emit(
                    Instruction::StaticCall {
                        class,
                        method,
                        binding,
                    },
                    line,
                );
                self.compile_arguments(arguments, line)
            }
            ast::ExpressionKind::InstanceOf { value, class } => {
                let source = self.compile_expression(value)?;
                let destination = self.allocate_register()?;
                if matches!(class.kind, ast::ExpressionKind::Name(_)) {
                    let class = self.object_class_name(class, line)?;
                    self.emit(
                        Instruction::InstanceOf {
                            destination,
                            source,
                            class,
                        },
                        line,
                    );
                } else {
                    let class = self.compile_expression(class)?;
                    self.emit(
                        Instruction::DynamicInstanceOf {
                            destination,
                            source,
                            class,
                        },
                        line,
                    );
                }
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
