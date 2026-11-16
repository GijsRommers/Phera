//! Bytecode lowering for expressions.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::Constant;
use crate::{CompileError, Compiler, constant};

impl Compiler<'_> {
    /// Dispatch an individual AST node after recognizing literal constants.
    pub(crate) fn compile_expression_node(
        &mut self,
        expression: &ast::Expression,
    ) -> Result<Reg, CompileError> {
        use ast::ExpressionKind;
        let line = self.lines.line(expression.span.start);
        if let Some(condition) = constant(expression) {
            return self.literal(condition, line);
        }
        match &expression.kind {
            ExpressionKind::Variable(_) | ExpressionKind::Index { .. } => {
                self.compile_read(expression, line)
            }
            ExpressionKind::Assign { target, value } => {
                self.compile_assignment(target, value, line)
            }
            ExpressionKind::AssignReference { target, value } => {
                self.compile_reference_assignment(target, value, line)
            }
            ExpressionKind::CompoundAssign {
                operator,
                target,
                value,
            } => self.compile_compound_assignment(operator, target, value, line),
            ExpressionKind::IncDec {
                target,
                increment,
                prefix,
            } => self.compile_increment(
                target,
                match (*increment, *prefix) {
                    (true, true) => phera_bytecode::IncDec::PreIncrement,
                    (true, false) => phera_bytecode::IncDec::PostIncrement,
                    (false, true) => phera_bytecode::IncDec::PreDecrement,
                    (false, false) => phera_bytecode::IncDec::PostDecrement,
                },
                line,
            ),
            ExpressionKind::Binary {
                operator: ast::BinaryOperator::Pipe,
                left,
                right,
            } => self.compile_pipe(left, right, line),
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } => self.compile_short_circuit(operator, left, right, line),
            ExpressionKind::Unary { operator, operand } => {
                self.compile_unary(operator, operand, line)
            }
            ExpressionKind::Cast { kind, operand } => self.compile_cast(kind, operand, line),
            ExpressionKind::Ternary {
                condition,
                then,
                otherwise,
            } => self.compile_ternary(condition, then, otherwise, line),
            ExpressionKind::Include { kind, path } => self.compile_include(kind, path, line),
            ExpressionKind::Print(expression) => self.compile_print(expression, line),
            ExpressionKind::Exit(expression) => self.compile_exit(expression, line),
            ExpressionKind::Name(name) => self.compile_constant(name, line),
            ExpressionKind::Call { callee, arguments } => {
                self.compile_call(callee, arguments, line)
            }
            ExpressionKind::Array(items) => self.compile_array(items, line),
            ExpressionKind::InterpolatedString(parts) => self.compile_interpolation(parts, line),
            ExpressionKind::Empty(value) => self.compile_empty(value, line),
            ExpressionKind::Isset(values) => self.compile_isset(values, line),
            ExpressionKind::Match(match_expression) => self.compile_match(match_expression, line),
            ExpressionKind::MagicConstant(match_expression) => {
                self.compile_magic_constant(match_expression, line)
            }
            _ => Err(self.error("Expression not supported yet", line)),
        }
    }
    /// Resolve magic constants from the current source and function context.
    fn compile_magic_constant(
        &mut self,
        match_expression: &ast::MagicConstant,
        line: usize,
    ) -> Result<Reg, CompileError> {
        self.literal(
            match match_expression {
                ast::MagicConstant::Line => Constant::Int(line as i64),
                ast::MagicConstant::File => Constant::Bytes(self.file.as_bytes().to_vec()),
                ast::MagicConstant::Dir => Constant::Bytes(
                    std::path::Path::new(self.file)
                        .parent()
                        .unwrap_or(std::path::Path::new("."))
                        .to_string_lossy()
                        .as_bytes()
                        .to_vec(),
                ),
                ast::MagicConstant::Function | ast::MagicConstant::Method => {
                    Constant::Bytes(if self.function.name == "{main}" {
                        vec![]
                    } else {
                        self.function.name.as_bytes().to_vec()
                    })
                }
                ast::MagicConstant::Namespace => {
                    Constant::Bytes(self.namespace.as_bytes().to_vec())
                }
                _ => Constant::Bytes(vec![]),
            },
            line,
        )
    }
}

pub(crate) mod arrays;
pub(crate) mod assignments;
pub(crate) mod calls;
pub(crate) mod conditionals;
pub(crate) mod matching;
pub(crate) mod operators;
pub(crate) mod paths;
pub(crate) mod places;
pub(crate) mod spine;
pub(crate) mod values;
