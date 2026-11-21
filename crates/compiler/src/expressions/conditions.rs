//! Compile a condition directly as the jumps that decide it.
//!
//! ```php
//! if ($a === $b) { ... }          // BinaryBranch { Identical, $a, $b, else, when: false }
//! while ($k < $n) { ... }         // BinaryIntBranch { Less, $k, $n, end, when: false }
//! if ($a !== null) { ... }        // JumpIfNull { $a, else }
//! if (!$a) { ... }                // JumpIf { $a, else }: no Move, no Unary Not
//! do { ... } while ($a && $b);    // two jumps, no boolean temporaries
//! ```
//!
//! A comparison whose value is used (not only branched on) still compiles to
//! `Binary` as before; this module only runs where the result decides a jump.
//! Operands are evaluated in PHP's order, left to right, whatever form the
//! branch takes, and an undefined variable still warns on the line PHP puts
//! the warning on.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Constant, Instruction, Target};
use crate::{CompileError, Compiler, binary, constant};

/// The comparison operators: their result is the boolean a branch tests.
fn is_comparison(operator: ast::BinaryOperator) -> bool {
    use ast::BinaryOperator::*;
    matches!(
        operator,
        Equal | NotEqual | Identical | NotIdentical | Less | LessEqual | Greater | GreaterEqual
    )
}

/// The same comparison with its operands swapped: `5 > $a` tests `$a < 5`.
/// Equality and identity are symmetric; the orderings pair up.
fn swapped(operator: ast::BinaryOperator) -> ast::BinaryOperator {
    use ast::BinaryOperator::*;
    match operator {
        Less => Greater,
        LessEqual => GreaterEqual,
        Greater => Less,
        GreaterEqual => LessEqual,
        other => other,
    }
}

impl Compiler<'_> {
    /// Compile `expression` as the jumps that decide a branch. Every returned
    /// patch position jumps to the caller's target when the expression's
    /// truthiness equals `when`. An operand that short-circuits the other way
    /// is patched past the expression here, so callers only patch what they
    /// were given.
    pub(crate) fn compile_condition(
        &mut self,
        expression: &ast::Expression,
        when: bool,
        line: usize,
    ) -> Result<Vec<usize>, CompileError> {
        match &expression.kind {
            ast::ExpressionKind::Binary {
                operator,
                left,
                right,
            } => match operator {
                ast::BinaryOperator::And => {
                    if when {
                        // Jump when both operands hold: a false left one ends it.
                        let left = self.compile_condition(left, false, line)?;
                        let right = self.compile_condition(right, true, line)?;
                        let after = self.position();
                        for patch in left {
                            self.patch(patch, after);
                        }
                        Ok(right)
                    } else {
                        let mut patches = self.compile_condition(left, false, line)?;
                        patches.extend(self.compile_condition(right, false, line)?);
                        Ok(patches)
                    }
                }
                ast::BinaryOperator::Or => {
                    if when {
                        let mut patches = self.compile_condition(left, true, line)?;
                        patches.extend(self.compile_condition(right, true, line)?);
                        Ok(patches)
                    } else {
                        // Jump when neither holds: a true left one ends it.
                        let left = self.compile_condition(left, true, line)?;
                        let right = self.compile_condition(right, false, line)?;
                        let after = self.position();
                        for patch in left {
                            self.patch(patch, after);
                        }
                        Ok(right)
                    }
                }
                operator
                    if is_comparison(*operator)
                        && !(constant(left).is_some() && constant(right).is_some()) =>
                {
                    self.compile_comparison(*operator, left, right, when, line)
                }
                _ => self.compile_value_condition(expression, when, line),
            },
            ast::ExpressionKind::Unary {
                operator: ast::UnaryOperator::Not,
                operand,
            } => self.compile_condition(operand, !when, line),
            _ => self.compile_value_condition(expression, when, line),
        }
    }

    /// Any other condition: evaluate it, then branch on the value. A plain
    /// local branches on its own register, without the copy a read makes.
    fn compile_value_condition(
        &mut self,
        expression: &ast::Expression,
        when: bool,
        line: usize,
    ) -> Result<Vec<usize>, CompileError> {
        let value = if let Some(local) = self.compiled_variable(expression)? {
            // The warning for an undefined local belongs to the variable's
            // own line, where the read that used to precede the branch was.
            let variable_line = self.lines.line(expression.span.start);
            return Ok(vec![self.emit(
                Instruction::Branch {
                    value: local,
                    target: Target(0),
                    when,
                },
                variable_line,
            )]);
        } else {
            self.compile_expression(expression)?
        };
        Ok(vec![self.emit(
            Instruction::Branch {
                value,
                target: Target(0),
                when,
            },
            line,
        )])
    }

    /// One comparison as a branch: the literal forms ride inside the
    /// instruction, the rest compare two registers.
    fn compile_comparison(
        &mut self,
        operator: ast::BinaryOperator,
        left: &ast::Expression,
        right: &ast::Expression,
        when: bool,
        line: usize,
    ) -> Result<Vec<usize>, CompileError> {
        let left_literal = constant(left);
        let right_literal = constant(right);
        // A plain local on either side reads its own register; the operator's
        // line follows the right operand, as in the expression spine.
        let left_local = self.compiled_variable(left)?;
        let right_local = self.compiled_variable(right)?;
        let operation_line = if left_local.is_some() || right_local.is_some() {
            self.operand_line(right)
        } else {
            line
        };
        // A small integer literal on the right carries inside the instruction.
        if left_literal.is_none()
            && let Some(Constant::Int(value)) = right_literal
            && let Ok(rhs) = i32::try_from(value)
        {
            let lhs = self.condition_operand(left, left_local)?;
            return Ok(vec![
                self.emit(
                    Instruction::BranchCompareInt {
                        operator: binary(operator)
                            .ok_or_else(|| self.error("Operator not supported yet", line))?,
                        left: lhs,
                        rhs,
                        target: Target(0),
                        when,
                    },
                    operation_line,
                ),
            ]);
        }
        // The same literal on the left: compare with the operands swapped.
        if right_literal.is_none()
            && let Some(Constant::Int(value)) = left_literal
            && let Ok(rhs) = i32::try_from(value)
        {
            let operand = self.condition_operand(right, right_local)?;
            return Ok(vec![
                self.emit(
                    Instruction::BranchCompareInt {
                        operator: binary(swapped(operator))
                            .ok_or_else(|| self.error("Operator not supported yet", line))?,
                        left: operand,
                        rhs,
                        target: Target(0),
                        when,
                    },
                    operation_line,
                ),
            ]);
        }
        // `=== null` and `!== null` reuse the null test itself.
        if matches!(
            operator,
            ast::BinaryOperator::Identical | ast::BinaryOperator::NotIdentical
        ) {
            let null_on_right =
                left_literal.is_none() && matches!(right_literal, Some(Constant::Null));
            let null_on_left =
                right_literal.is_none() && matches!(left_literal, Some(Constant::Null));
            if null_on_right || null_on_left {
                let (operand_expression, operand_local) = if null_on_right {
                    (left, left_local)
                } else {
                    (right, right_local)
                };
                let value = self.condition_operand(operand_expression, operand_local)?;
                // `$x === null` jumps when null exactly when `when` holds;
                // `!==` is its negation.
                let jump_when_null = (operator == ast::BinaryOperator::Identical) == when;
                let instruction = if jump_when_null {
                    Instruction::BranchNull {
                        value,
                        target: Target(0),
                    }
                } else {
                    Instruction::BranchNotNull {
                        value,
                        target: Target(0),
                    }
                };
                return Ok(vec![self.emit(instruction, operation_line)]);
            }
        }
        // Any other comparison reads both operands in source order.
        let lhs = self.condition_operand(left, left_local)?;
        let rhs = self.condition_operand(right, right_local)?;
        Ok(vec![
            self.emit(
                Instruction::BranchCompare {
                    operator: binary(operator)
                        .ok_or_else(|| self.error("Operator not supported yet", line))?,
                    left: lhs,
                    right: rhs,
                    target: Target(0),
                    when,
                },
                operation_line,
            ),
        ])
    }

    /// One comparison operand: a plain local's register, a literal's load, or
    /// the expression's own code, in PHP's evaluation order.
    fn condition_operand(
        &mut self,
        expression: &ast::Expression,
        local: Option<Reg>,
    ) -> Result<Reg, CompileError> {
        if let Some(local) = local {
            return Ok(local);
        }
        if let Some(value) = constant(expression) {
            let line = self.lines.line(expression.span.start);
            return self.literal(value, line);
        }
        self.compile_expression(expression)
    }
}
