//! Lower AST expression constructs to IR while preserving PHP evaluation order.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Constant, Instruction, Target, Unary};
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Convert the evaluated operand using PHP cast semantics.
    pub(crate) fn compile_cast(
        &mut self,
        kind: &ast::CastKind,
        operand: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let operator = match kind {
            ast::CastKind::Int => Unary::Int,
            ast::CastKind::Float => Unary::Float,
            ast::CastKind::String => Unary::String,
            ast::CastKind::Bool => Unary::Bool,
            ast::CastKind::Array => Unary::Array,
            ast::CastKind::Object => Unary::Object,
            _ => return Err(self.error("Cast not supported yet", line)),
        };
        let src = self.compile_expression(operand)?;
        let dst = self.allocate_register()?;
        self.emit(
            Instruction::Unary {
                destination: dst,
                operator,
                source: src,
            },
            line,
        );
        Ok(dst)
    }

    /// Evaluate only the selected branch, retaining shorthand ternary values.
    pub(crate) fn compile_ternary(
        &mut self,
        condition: &ast::Expression,
        then: &Option<Box<ast::Expression>>,
        otherwise: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let condition = self.compile_expression(condition)?;
        let dst = self.allocate_register()?;
        self.emit(
            Instruction::Move {
                destination: dst,
                source: condition,
            },
            line,
        );
        let jump = self.emit(
            Instruction::Branch {
                value: condition,
                target: Target(0),
                when: false,
            },
            line,
        );
        if let Some(then_expression) = then {
            let src = self.compile_expression(then_expression)?;
            self.emit(
                Instruction::Move {
                    destination: dst,
                    source: src,
                },
                line,
            );
        }
        let end = self.emit(Instruction::Jump(Target(0)), line);
        self.patch(jump, self.position());
        let src = self.compile_expression(otherwise)?;
        self.emit(
            Instruction::Move {
                destination: dst,
                source: src,
            },
            line,
        );
        self.patch(end, self.position());
        Ok(dst)
    }

    /// An evaluated string can observe and modify any caller variable.
    pub(crate) fn compile_eval(&mut self, code: &ast::Expression) -> Result<Reg, CompileError> {
        self.function.dynamic_scope = true;
        let source = self.compile_expression(code)?;
        let destination = self.allocate_register()?;
        // Zend emits eval after visiting its operand. Heredoc's first string
        // token starts after the opener, unlike an ordinary quoted literal.
        let text = self
            .source
            .get(code.span.start..code.span.end)
            .unwrap_or_default();
        let content = text
            .strip_prefix(b"b")
            .or_else(|| text.strip_prefix(b"B"))
            .unwrap_or(text);
        let line = self.lines.line(code.span.start) + usize::from(content.starts_with(b"<<<"));
        self.emit(
            Instruction::Eval {
                destination,
                source,
            },
            line,
        );
        Ok(destination)
    }

    /// Includes need dynamic scope because included code sees local variables.
    pub(crate) fn compile_include(
        &mut self,
        kind: &ast::IncludeKind,
        path: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        self.function.dynamic_scope = true;
        let path = self.compile_expression(path)?;
        let dst = self.allocate_register()?;
        self.emit(
            Instruction::Include {
                destination: dst,
                path,
                once: matches!(
                    kind,
                    ast::IncludeKind::IncludeOnce | ast::IncludeKind::RequireOnce
                ),
                required: matches!(
                    kind,
                    ast::IncludeKind::Require | ast::IncludeKind::RequireOnce
                ),
            },
            line,
        );
        Ok(dst)
    }

    /// Print the expression and produce PHP print's integer result.
    pub(crate) fn compile_print(
        &mut self,
        expression: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let register = self.compile_expression(expression)?;
        self.emit(Instruction::Echo(register), line);
        self.literal(Constant::Int(1), line)
    }
}
