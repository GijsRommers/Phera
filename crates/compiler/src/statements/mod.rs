//! Bytecode lowering for statements.

use phera_ast as ast;

use crate::policies::DeclarationTiming;
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Dispatch a statement with its source location and declaration timing.
    pub(crate) fn statement(
        &mut self,
        statement: &ast::Statement,
        timing: DeclarationTiming,
    ) -> Result<(), CompileError> {
        use ast::StatementKind;
        let line = self.lines.line(statement.span.start);
        match &statement.kind {
            StatementKind::Expression(expression) => {
                self.compile_expression_statement(expression, line)?
            }
            StatementKind::Echo(expressions) => self.compile_echo(expressions, line)?,
            StatementKind::InlineHtml(value) => self.compile_inline_html(value, line)?,
            StatementKind::Block(body) => self.block(body, DeclarationTiming::AtExecution)?,
            StatementKind::If {
                condition,
                then,
                otherwise,
            } => self.compile_if(condition, then, otherwise, line)?,
            StatementKind::While { condition, body } => {
                self.compile_while(condition, body, line)?
            }
            StatementKind::DoWhile { body, condition } => {
                self.compile_do_while(body, condition, line)?
            }
            StatementKind::For {
                init,
                condition,
                step,
                body,
            } => self.compile_for(init, condition, step, body, line)?,
            StatementKind::Break(name) | StatementKind::Continue(name) => {
                self.compile_loop_jump(statement, name, line)?
            }
            StatementKind::Goto(label) => self.compile_goto(label),
            StatementKind::Label(label) => self.compile_label(label)?,
            StatementKind::Try {
                body,
                catches,
                finally,
            } => self.compile_try(body, catches, finally, line)?,
            StatementKind::Return(expression) => self.compile_return(expression, line)?,
            StatementKind::Class(declaration) => self.compile_class(declaration, timing, line)?,
            StatementKind::Function(declaration) => {
                self.compile_function(declaration, timing, line)?
            }
            StatementKind::Namespace {
                name, statements, ..
            } => self.compile_namespace(name, statements, timing)?,
            StatementKind::Use(items) => self.compile_use(items)?,
            StatementKind::Const { items, .. } => self.compile_constants(items, line)?,
            StatementKind::Declare { directives, body } => {
                self.compile_declare(directives, body, timing, line)?
            }
            StatementKind::Global(variables) => self.compile_globals(variables, line)?,
            StatementKind::Static(variables) => self.compile_statics(variables, line)?,
            StatementKind::Unset(expressions) => self.compile_unset(expressions, line)?,
            StatementKind::Foreach {
                subject,
                key,
                value,
                by_reference,
                body,
            } => self.compile_foreach(
                subject,
                key,
                value,
                if *by_reference {
                    crate::policies::IterationBinding::Reference
                } else {
                    crate::policies::IterationBinding::Value
                },
                body,
                line,
            )?,
            StatementKind::Switch { subject, cases } => {
                self.compile_switch(subject, cases, line)?
            }
            _ => return Err(self.error("Statement not supported yet", line)),
        }
        Ok(())
    }
}

pub(crate) mod bindings;
pub(crate) mod classes;
pub(crate) mod control;
pub(crate) mod declarations;
mod function_parameters;
pub(crate) mod functions;
pub(crate) mod loops;
mod magic_methods;
pub(crate) mod switching;

pub(crate) mod exceptions;
pub(crate) mod exits;
pub(crate) mod gotos;

pub(crate) mod class_errors;
