//! Object operations preserve callee-before-arguments ordering through the existing call protocol.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::Instruction;
use crate::policies::{DeclarationTiming, NameKind};
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// PHP reports a method call, and errors about it, on the line of the
    /// method's name. In a chain written over several lines that is not the
    /// line the expression starts on.
    pub(super) fn member_line(&self, name: &ast::MemberName) -> usize {
        let start = match name {
            ast::MemberName::Identifier(name) => name.span.start,
            ast::MemberName::Expression(name) => name.span.start,
        };
        self.lines.line(start)
    }

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
            } if is_named_class(class) => {
                let class = self.object_class_name(class, line)?;
                let destination = self.allocate_register()?;
                self.emit(
                    Instruction::NamedClassConstant {
                        destination,
                        class,
                        name: name.name.clone(),
                    },
                    line,
                );
                Ok(destination)
            }
            ast::ExpressionKind::ClassConstant {
                class,
                name: ast::MemberName::Identifier(name),
            } => {
                // `self::K` / `parent::K` in a method of a named class: the
                // class is known here, so read the constant by its symbol.
                if let Some(class_name) = self.relative_class_name(class) {
                    let destination = self.allocate_register()?;
                    self.emit(
                        Instruction::NamedClassConstant {
                            destination,
                            class: class_name,
                            name: name.name.clone(),
                        },
                        line,
                    );
                    return Ok(destination);
                }
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
                class: ast::NewTarget::Anonymous(declaration),
                arguments,
            } => {
                if matches!(arguments, ast::Arguments::FirstClassCallable) {
                    return Err(self.error("Cannot create Closure for new expression", line));
                }
                let class_line = self.lines.line(declaration.span.start);
                let class =
                    self.compile_class(declaration, DeclarationTiming::AtExecution, class_line)?;
                self.emit(Instruction::New { class }, class_line);
                self.compile_arguments(arguments, class_line)
            }
            ast::ExpressionKind::New {
                class: ast::NewTarget::Class(class),
                arguments,
            } => {
                if matches!(arguments, ast::Arguments::FirstClassCallable) {
                    return Err(self.error("Cannot create Closure for new expression", line));
                }
                let line = self.lines.line(class.span.end.saturating_sub(1));
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
                let receiver = self.compile_receiver(target)?;
                let line = self.member_line(name);
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
            ast::ExpressionKind::Clone { object, with } => {
                let source = self.compile_expression(object)?;
                let with = with
                    .as_ref()
                    .map(|with| self.compile_expression(with))
                    .transpose()?;
                let destination = self.allocate_register()?;
                self.emit(
                    Instruction::Clone {
                        destination,
                        source,
                        with,
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
                let line = self.member_line(name);
                if let Some(result) = self.compile_parent_hook(class, name, arguments, line)? {
                    return Ok(result);
                }
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

    /// The name a `self::` / `parent::` constant target resolves to at compile
    /// time. Only a method body of a named, non-trait class qualifies: in a
    /// trait `self` is the class that uses the trait, in a closure or arrow
    /// function it can be rebound, an anonymous class keeps the general form,
    /// and an initializer does too, so the self-referencing-constant message
    /// keeps the written `self::`. `static` is only known at run time.
    fn relative_class_name(&self, class: &ast::Expression) -> Option<String> {
        let ast::ExpressionKind::Name(name) = &class.kind else {
            return None;
        };
        let keyword = name.text.to_ascii_lowercase();
        if !matches!(keyword.as_str(), "self" | "parent") {
            return None;
        }
        if self.trait_scope.is_some()
            || !self.function.name.contains("::")
            || self.function.name.starts_with("{closure:")
        {
            return None;
        }
        let scope = self.class_scope.as_deref()?;
        if scope.contains('\0') {
            return None;
        }
        match keyword.as_str() {
            "self" => Some(scope.to_owned()),
            _ => self.parent_scope.clone(),
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

impl Compiler<'_> {
    /// The register of a method call's receiver. `$object->method()` reads
    /// `$object` when the call starts (`InitMethodCall` reads it then, with
    /// the same warning for an undefined variable), so a plain local is
    /// used as it is, without a copy that would then have to be released.
    fn compile_receiver(&mut self, target: &ast::Expression) -> Result<Reg, CompileError> {
        // In a long chain the receiver may be compiled already (see
        // `expressions::chains`): it is taken from there.
        if let ast::ExpressionKind::Variable(ast::Variable::Named(name)) = &target.kind
            && name != "this"
            && !self.is_compiled_operand(target)
        {
            let place = self.readable_place(target)?;
            if place.scope == crate::ir::PlaceScope::Local && place.path.is_empty() {
                return Ok(place.base);
            }
            let line = self.lines.line(target.span.start);
            let destination = self.allocate_register()?;
            self.emit(
                Instruction::Read {
                    destination,
                    place,
                    mode: phera_bytecode::FetchMode::Read,
                },
                line,
            );
            return Ok(destination);
        }
        self.compile_expression(target)
    }
}

/// Whether `class` names a class in the source (`Post`, not `self`,
/// `parent`, `static` or an expression): its constants are looked up by the
/// class's symbol.
pub(crate) fn is_named_class(class: &ast::Expression) -> bool {
    match &class.kind {
        ast::ExpressionKind::Name(name) => !matches!(
            name.text.to_ascii_lowercase().as_str(),
            "self" | "parent" | "static"
        ),
        _ => false,
    }
}
