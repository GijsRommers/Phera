//! Iterative expression-spine lowering avoids Rust recursion for long PHP chains.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Argument, Constant, Instruction, Target, Unary};
use crate::{CompileError, Compiler, binary};
/// One deferred right operand, retaining its operator and source location.
struct BinaryStep<'ast> {
    operator: ast::BinaryOperator,
    right: &'ast ast::Expression,
    line: usize,
}

/// Whether compiling `operand` fills registers only with numbers, booleans,
/// null or strings: it is made of literals, joined by operators, prefix
/// operators and casts to those types. `$i * 2` qualifies through its `2`;
/// `$a === $b` does not, since the copies of `$a` and `$b` may be objects.
fn holds_only_scalars(operand: &ast::Expression) -> bool {
    let mut pending = vec![operand];
    while let Some(node) = pending.pop() {
        match &node.kind {
            ast::ExpressionKind::Literal(_) => {}
            ast::ExpressionKind::Binary { left, right, .. } => {
                pending.push(left);
                pending.push(right);
            }
            ast::ExpressionKind::Unary { operator, operand }
                if *operator != ast::UnaryOperator::ErrorSuppress =>
            {
                pending.push(operand)
            }
            ast::ExpressionKind::Cast {
                kind:
                    ast::CastKind::Int
                    | ast::CastKind::Float
                    | ast::CastKind::Bool
                    | ast::CastKind::String,
                operand,
            } => pending.push(operand),
            _ => return false,
        }
    }
    true
}

/// Joins the string literals a concatenation chain starts with, as PHP does
/// while compiling: `'a' . 'b' . $c` loads `'ab'` and appends `$c`. The
/// folded steps are taken off `spine` (top first, so the start is its end).
///
/// Generated templates concatenate thousands of literal pieces; appending
/// them one by one at run time would copy the growing string each time.
fn fold_leading_strings(
    start: &ast::Expression,
    spine: &mut Vec<BinaryStep<'_>>,
) -> Option<Vec<u8>> {
    let ast::ExpressionKind::Literal(ast::Literal::String(first)) = &start.kind else {
        return None;
    };
    let mut bytes = None;
    while let Some(step) = spine.last()
        && step.operator == ast::BinaryOperator::Concat
        && let ast::ExpressionKind::Literal(ast::Literal::String(piece)) = &step.right.kind
    {
        bytes
            .get_or_insert_with(|| first.clone())
            .extend_from_slice(piece);
        spine.pop();
    }
    bytes
}

impl Compiler<'_> {
    /// Walk long left-associated binary chains without growing the Rust call stack.
    pub(crate) fn compile_expression(
        &mut self,
        expression: &ast::Expression,
    ) -> Result<Reg, CompileError> {
        if let Some(register) = self.take_compiled_operand(expression) {
            return Ok(register);
        }
        if matches!(expression.kind, ast::ExpressionKind::Binary { .. }) {
            let mut spine = vec![];
            let mut node = expression;
            while let ast::ExpressionKind::Binary {
                operator,
                left,
                right,
            } = &node.kind
            {
                spine.push(BinaryStep {
                    operator: *operator,
                    right,
                    line: self.lines.line(node.span.start),
                });
                node = left;
            }
            let folded = fold_leading_strings(node, &mut spine);
            // Backing storage is a lexical property of a hook, including references
            // in code that can never run.
            let mut known_truth = self
                .hooked_property
                .is_none()
                .then(|| match &folded {
                    Some(bytes) => Some(!bytes.is_empty() && bytes != b"0"),
                    None => literal_truth(node),
                })
                .flatten();
            // A run of `false && ...` steps keeps one literal result.
            let mut decided = false;
            let mut first_local = if spine.last().is_some_and(|step| {
                !matches!(
                    step.operator,
                    ast::BinaryOperator::And
                        | ast::BinaryOperator::Or
                        | ast::BinaryOperator::NullCoalesce
                        | ast::BinaryOperator::Pipe
                )
            }) {
                self.compiled_variable(node)?
            } else {
                None
            };
            let mut lhs = if let Some(bytes) = folded {
                self.literal(Constant::Bytes(bytes), self.lines.line(node.span.start))?
            } else if first_local.is_some() {
                self.allocate_register()?
            } else if spine
                .last()
                .is_some_and(|step| step.operator == ast::BinaryOperator::NullCoalesce)
            {
                self.quiet(node)?
            } else {
                self.compile_expression(node)?
            };
            for BinaryStep {
                operator,
                right,
                line,
            } in spine.into_iter().rev()
            {
                if matches!(
                    (operator, known_truth),
                    (ast::BinaryOperator::And, Some(false)) | (ast::BinaryOperator::Or, Some(true))
                ) {
                    if !std::mem::replace(&mut decided, true) {
                        lhs = self
                            .literal(Constant::Bool(operator == ast::BinaryOperator::Or), line)?;
                    }
                    continue;
                }
                known_truth = None;
                decided = false;
                let mark = self.mark_temporaries();
                match operator {
                    ast::BinaryOperator::And
                    | ast::BinaryOperator::Or
                    | ast::BinaryOperator::NullCoalesce => {
                        lhs = self.compile_short_circuit_step(operator, lhs, right, line)?;
                    }
                    ast::BinaryOperator::Pipe => {
                        lhs = self.compile_pipe_step(operator, lhs, right, line)?;
                    }
                    _ => {
                        let right_local = self.compiled_variable(right)?;
                        let rhs = match right_local {
                            Some(local) => local,
                            None => self.compile_expression(right)?,
                        };
                        let operation_line = if first_local.is_some() || right_local.is_some() {
                            self.operand_line(right)
                        } else {
                            line
                        };
                        self.emit(
                            Instruction::Binary {
                                destination: lhs,
                                operator: binary(operator).ok_or_else(|| {
                                    self.error("Operator not supported yet", line)
                                })?,
                                left: first_local.take().unwrap_or(lhs),
                                right: rhs,
                            },
                            operation_line,
                        );
                    }
                }
                if holds_only_scalars(right) {
                    self.release_scalar_temporaries(mark);
                } else {
                    self.release_temporaries(mark);
                }
            }
            return Ok(lhs);
        }
        if let Some(result) = self.compile_long_chain(expression) {
            return result;
        }
        self.compile_expression_node(expression)
    }
    /// Evaluate one deferred chain operand without recursing down the left spine.
    fn compile_short_circuit_step(
        &mut self,
        operator: ast::BinaryOperator,
        lhs: Reg,
        right: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let dst = lhs;
        let jump = if operator == ast::BinaryOperator::NullCoalesce {
            let jump = self.emit(
                Instruction::BranchNull {
                    value: lhs,
                    target: Target(0),
                },
                line,
            );
            let end = self.emit(Instruction::Jump(Target(0)), line);
            self.patch(jump, self.position());
            end
        } else {
            self.emit(
                Instruction::Branch {
                    value: lhs,
                    target: Target(0),
                    when: operator == ast::BinaryOperator::Or,
                },
                line,
            )
        };
        let src = self.compile_expression(right)?;
        self.emit(
            Instruction::Move {
                destination: dst,
                source: src,
            },
            line,
        );
        self.patch(jump, self.position());
        if operator != ast::BinaryOperator::NullCoalesce {
            self.emit(
                Instruction::Unary {
                    destination: dst,
                    operator: Unary::Bool,
                    source: dst,
                },
                line,
            );
        }
        Ok(dst)
    }

    /// Evaluate one deferred chain operand without recursing down the left spine.
    fn compile_pipe_step(
        &mut self,
        _operator: ast::BinaryOperator,
        lhs: Reg,
        right: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let name = self.compile_expression(right)?;
        let dst = lhs;
        self.emit(
            Instruction::Call {
                result: phera_bytecode::CallResult::Used,
                destination: dst,
                name,
                fallback: None,
                arguments: vec![Argument {
                    source: crate::ir::ArgumentSource::Value(lhs),
                    name: None,
                    spread: false,
                    of_call: false,
                }],
            },
            line,
        );
        Ok(dst)
    }
}

fn literal_truth(expression: &ast::Expression) -> Option<bool> {
    match &expression.kind {
        ast::ExpressionKind::Literal(literal) => Some(match literal {
            ast::Literal::Null => false,
            ast::Literal::Bool(value) => *value,
            ast::Literal::Int(value) => *value != 0,
            ast::Literal::Float(value) => *value != 0.0,
            ast::Literal::String(value) => !value.is_empty() && value != b"0",
        }),
        ast::ExpressionKind::Array(values) if values.is_empty() => Some(false),
        _ => None,
    }
}
