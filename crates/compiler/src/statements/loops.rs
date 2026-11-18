//! Compile statement constructs into ordered IR and patch their control-flow exits.

use phera_ast as ast;
use phera_ast::StatementKind;

use crate::ir::{Constant, Instruction, Target};
use crate::policies::DeclarationTiming;
use crate::{CompileError, Compiler, Loop};

impl Compiler<'_> {
    /// Run the body before testing, with continue targeting the test.
    pub(crate) fn compile_do_while(
        &mut self,
        body: &[ast::Statement],
        condition: &ast::Expression,
        line: usize,
    ) -> Result<(), CompileError> {
        let start = self.position();
        self.loops.push(Loop {
            breaks: vec![],
            continues: vec![],
        });
        self.block(body, DeclarationTiming::AtExecution)?;
        let continue_at = self.position();
        let register = self.compile_expression(condition)?;
        self.emit(
            Instruction::Branch {
                value: register,
                target: Target(start),
                when: true,
            },
            line,
        );
        self.loop_end(continue_at);

        Ok(())
    }

    /// Only the last condition controls the loop; continue runs the step.
    pub(crate) fn compile_for(
        &mut self,
        init: &[ast::Expression],
        condition: &[ast::Expression],
        step: &[ast::Expression],
        body: &[ast::Statement],
        line: usize,
    ) -> Result<(), CompileError> {
        for expression in init {
            self.compile_expression(expression)?;
        }
        let start = self.position();
        let mut last = None;
        for expression in condition {
            last = Some(self.compile_expression(expression)?)
        }
        let end = last.map(|register| {
            self.emit(
                Instruction::Branch {
                    value: register,
                    target: Target(0),
                    when: false,
                },
                line,
            )
        });
        self.loops.push(Loop {
            breaks: vec![],
            continues: vec![],
        });
        self.block(body, DeclarationTiming::AtExecution)?;
        let continue_at = self.position();
        for expression in step {
            self.compile_expression(expression)?;
        }
        self.emit(Instruction::Jump(Target(start)), line);
        if let Some(jump) = end {
            self.patch(jump, self.position())
        }
        self.loop_end(continue_at);

        Ok(())
    }

    /// Count outward through loop contexts to implement break and continue levels.
    pub(crate) fn compile_loop_jump(
        &mut self,
        statement: &ast::Statement,
        name: &u32,
        line: usize,
    ) -> Result<(), CompileError> {
        let is_break = matches!(statement.kind, StatementKind::Break(_));
        let depth = *name as usize;
        if depth == 0 || depth > self.loops.len() {
            return Err(self.error(
                if self.loops.is_empty() {
                    format!(
                        "'{}' not in the 'loop' or 'switch' context",
                        if is_break { "break" } else { "continue" }
                    )
                } else {
                    format!("Cannot 'break' {name} levels")
                },
                line,
            ));
        }
        let at = self.loops.len() - depth;
        if self.finalies.iter().any(|scope| {
            matches!(
                scope.phase,
                crate::statements::exceptions::FinallyPhase::Running
            ) && scope.loop_depth > at
        }) {
            return Err(self.error("jump out of a finally block is disallowed", line));
        }
        self.emit_finally_exits(
            Some(at),
            if is_break {
                phera_bytecode::FinallyEntry::Break
            } else {
                phera_bytecode::FinallyEntry::Continue
            },
            line,
        );
        let jump = self.emit(Instruction::Jump(Target(0)), line);
        if let Some(loop_state) = self.loops.get_mut(at) {
            if is_break {
                loop_state.breaks.push(jump)
            } else {
                loop_state.continues.push(jump)
            }
        }

        Ok(())
    }

    /// Return a reference only when the function declaration requires one.
    pub(crate) fn compile_return(
        &mut self,
        expression: Option<&ast::Expression>,
        line: usize,
    ) -> Result<(), CompileError> {
        if let Some(return_type) = &self.function.return_type {
            let message = match (return_type.as_str(), expression.is_some()) {
                ("never", _) => Some("A never-returning function must not return"),
                ("void", true)
                    if expression.as_ref().is_some_and(|expression| {
                        matches!(
                            expression.kind,
                            ast::ExpressionKind::Literal(ast::Literal::Null)
                        )
                    }) =>
                {
                    Some(
                        "A void function must not return a value (did you mean \"return;\" instead of \"return null;\"?)",
                    )
                }
                ("void", true) => Some("A void function must not return a value"),
                ("void", false) | (_, true) => None,
                ("mixed", false) => Some(
                    "A function with return type must return a value (did you mean \"return null;\" instead of \"return;\"?)",
                ),
                (name, false) if name.contains("null") || name.starts_with('?') => Some(
                    "A function with return type must return a value (did you mean \"return null;\" instead of \"return;\"?)",
                ),
                (_, false) => Some("A function with return type must return a value"),
            };
            if let Some(message) = message {
                let message = if self.hooked_property.is_some()
                    || self
                        .function
                        .name
                        .rsplit_once("::")
                        .is_some_and(|(_, method)| method.eq_ignore_ascii_case("__toString"))
                {
                    message.replace("function", "method")
                } else {
                    message.into()
                };
                return Err(self.error(message, line));
            }
        }
        let source = if let Some(expression) = expression {
            if !self.function.returns_reference
                && self.finalies.is_empty()
                && let ast::ExpressionKind::Variable(ast::Variable::Named(name)) = &expression.kind
                && name != "this"
            {
                self.local(name)?
            } else if self.function.returns_reference
                && crate::helpers::reference_capable(expression)
            {
                let place = self.readable_place(expression)?;
                let dst = self.allocate_register()?;
                self.emit(
                    Instruction::Reference {
                        destination: dst,
                        place,
                    },
                    line,
                );
                dst
            } else {
                self.compile_expression(expression)?
            }
        } else {
            self.literal(Constant::Null, line)?
        };
        let source = if !self.finalies.is_empty() && !self.function.returns_reference {
            let destination = self.allocate_register()?;
            self.emit(
                Instruction::Move {
                    destination,
                    source,
                },
                line,
            );
            destination
        } else {
            source
        };
        if !self.finalies.is_empty() {
            self.emit(Instruction::CheckReturn { source }, line);
        }
        self.emit_finally_exits(None, phera_bytecode::FinallyEntry::Return, line);
        self.emit(
            if !self.finalies.is_empty() {
                Instruction::ReturnChecked { source }
            } else if self.function.returns_reference {
                Instruction::ReturnReference(source)
            } else {
                Instruction::Return(source)
            },
            line,
        );

        Ok(())
    }

    /// Restore the enclosing namespace after compiling its body.
    pub(crate) fn compile_namespace(
        &mut self,
        name: &Option<ast::Name>,
        statements: &[ast::Statement],
        timing: DeclarationTiming,
    ) -> Result<(), CompileError> {
        let old = self.namespace.clone();
        self.namespace = name
            .as_ref()
            .map(|name| name.text.clone())
            .unwrap_or_default();
        self.block(statements, timing)?;
        self.namespace = old;

        Ok(())
    }
}
