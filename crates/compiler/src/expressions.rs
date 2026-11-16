//! Bytecode lowering for expressions.
use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Argument, Constant, Instruction, Target, Unary};
use crate::{CompileError, Compiler, binary, constant, writable};
impl Compiler<'_> {
    pub(super) fn expr_node(&mut self, expression: &ast::Expression) -> Result<Reg, CompileError> {
        use ast::ExpressionKind as E;
        let line = self.lines.line(expression.span.start);
        if let Some(condition) = constant(expression) {
            return self.literal(condition, line);
        }
        match &expression.kind {
            E::Variable(_) | E::Index { .. } => {
                let place = self.readable_place(expression)?;
                let dst = self.reg()?;
                self.emit(
                    Instruction::Read {
                        dst,
                        place,
                        quiet: false,
                    },
                    line,
                );
                Ok(dst)
            }
            E::Assign { target, value } => {
                let references = matches!(&target.kind, E::List(items) if items.iter().flatten().any(|item| item.by_reference));
                let src = if references && writable(value) {
                    let place = self.place(value)?;
                    let dst = self.reg()?;
                    self.emit(Instruction::Reference { dst, place }, line);
                    dst
                } else {
                    self.expr(value)?
                };
                self.assign(target, src, line)?;
                Ok(src)
            }
            E::AssignReference { target, value } => {
                let place = self.place(target)?;
                if matches!(value.kind, ast::ExpressionKind::Call { .. }) {
                    let src = self.expr(value)?;
                    self.emit(Instruction::Rebind { place, src }, line);
                    return Ok(src);
                }
                let source = self.place(value)?;
                self.emit(
                    Instruction::Bind {
                        place: place.clone(),
                        source,
                    },
                    line,
                );
                let dst = self.reg()?;
                self.emit(
                    Instruction::Read {
                        dst,
                        place,
                        quiet: false,
                    },
                    line,
                );
                Ok(dst)
            }
            E::CompoundAssign {
                operator,
                target,
                value,
            } => {
                if *operator != ast::BinaryOperator::NullCoalesce {
                    let place = self.place(target)?;
                    let source = self.expr(value)?;
                    let dst = self.reg()?;
                    let operator = binary(*operator)
                        .ok_or_else(|| self.error("Operator not supported yet", line))?;
                    self.emit(
                        Instruction::Compound {
                            dst,
                            place,
                            operator,
                            source,
                        },
                        line,
                    );
                    return Ok(dst);
                }
                let place = self.place(target)?;
                let lhs = self.reg()?;
                self.emit(
                    Instruction::Read {
                        dst: lhs,
                        place: place.clone(),
                        quiet: *operator == ast::BinaryOperator::NullCoalesce,
                    },
                    line,
                );
                if *operator == ast::BinaryOperator::NullCoalesce {
                    let jump = self.emit(
                        Instruction::BranchNull {
                            value: lhs,
                            target: Target(0),
                        },
                        line,
                    );
                    let end = self.emit(Instruction::Jump(Target(0)), line);
                    self.patch(jump, self.pos());
                    let rhs = self.expr(value)?;
                    self.emit(Instruction::Write { place, src: rhs }, line);
                    self.emit(Instruction::Move { dst: lhs, src: rhs }, line);
                    self.patch(end, self.pos());
                    return Ok(lhs);
                }
                let rhs = self.expr(value)?;
                let dst = self.reg()?;
                self.emit(
                    Instruction::Binary {
                        dst,
                        operator: binary(*operator)
                            .ok_or_else(|| self.error("Operator not supported yet", line))?,
                        lhs,
                        rhs,
                    },
                    line,
                );
                self.emit(Instruction::Write { place, src: dst }, line);
                Ok(dst)
            }
            E::IncDec {
                target,
                increment,
                prefix,
            } => {
                let place = self.place(target)?;
                let dst = self.reg()?;
                self.emit(
                    Instruction::IncPlace {
                        dst,
                        place,
                        increment: *increment,
                        prefix: *prefix,
                    },
                    line,
                );
                Ok(dst)
            }
            E::Binary {
                operator: ast::BinaryOperator::Pipe,
                left,
                right,
            } => {
                let value = self.expr(left)?;
                let name = self.expr(right)?;
                let dst = self.reg()?;
                self.emit(
                    Instruction::Call {
                        dst,
                        name,
                        fallback: None,
                        arguments: vec![Argument {
                            value,
                            place: None,
                            name: None,
                            spread: false,
                        }],
                    },
                    line,
                );
                Ok(dst)
            }
            E::Binary {
                operator,
                left,
                right,
            } => {
                let lhs = if *operator == ast::BinaryOperator::NullCoalesce {
                    self.quiet(left)?
                } else {
                    self.expr(left)?
                };
                let dst = self.reg()?;
                self.emit(Instruction::Move { dst, src: lhs }, line);
                let jump = if *operator == ast::BinaryOperator::NullCoalesce {
                    let jump = self.emit(
                        Instruction::BranchNull {
                            value: lhs,
                            target: Target(0),
                        },
                        line,
                    );
                    let end = self.emit(Instruction::Jump(Target(0)), line);
                    self.patch(jump, self.pos());
                    end
                } else {
                    self.emit(
                        Instruction::Branch {
                            value: lhs,
                            target: Target(0),
                            when: *operator == ast::BinaryOperator::Or,
                        },
                        line,
                    )
                };
                let rhs = self.expr(right)?;
                self.emit(Instruction::Move { dst, src: rhs }, line);
                self.patch(jump, self.pos());
                if *operator != ast::BinaryOperator::NullCoalesce {
                    self.emit(
                        Instruction::Unary {
                            dst,
                            operator: Unary::Bool,
                            src: dst,
                        },
                        line,
                    );
                }
                Ok(dst)
            }
            E::Unary { operator, operand } => {
                if *operator == ast::UnaryOperator::ErrorSuppress {
                    self.emit(Instruction::Suppress(true), line);
                    let register = self.expr(operand)?;
                    self.emit(Instruction::Suppress(false), line);
                    return Ok(register);
                }
                let src = self.expr(operand)?;
                let dst = self.reg()?;
                let operator = match operator {
                    ast::UnaryOperator::Plus => Unary::Plus,
                    ast::UnaryOperator::Minus => Unary::Minus,
                    ast::UnaryOperator::Not => Unary::Not,
                    _ => Unary::BitwiseNot,
                };
                self.emit(Instruction::Unary { dst, operator, src }, line);
                Ok(dst)
            }
            E::Cast { kind, operand } => {
                let operator = match kind {
                    ast::CastKind::Int => Unary::Int,
                    ast::CastKind::Float => Unary::Float,
                    ast::CastKind::String => Unary::String,
                    ast::CastKind::Bool => Unary::Bool,
                    ast::CastKind::Array => Unary::Array,
                    _ => return Err(self.error("Cast not supported yet", line)),
                };
                let src = self.expr(operand)?;
                let dst = self.reg()?;
                self.emit(Instruction::Unary { dst, operator, src }, line);
                Ok(dst)
            }
            E::Ternary {
                condition,
                then,
                otherwise,
            } => {
                let condition = self.expr(condition)?;
                let dst = self.reg()?;
                self.emit(
                    Instruction::Move {
                        dst,
                        src: condition,
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
                    let src = self.expr(then_expression)?;
                    self.emit(Instruction::Move { dst, src }, line);
                }
                let end = self.emit(Instruction::Jump(Target(0)), line);
                self.patch(jump, self.pos());
                let src = self.expr(otherwise)?;
                self.emit(Instruction::Move { dst, src }, line);
                self.patch(end, self.pos());
                Ok(dst)
            }
            E::Include { kind, path } => {
                self.fun.dynamic_scope = true;
                let path = self.expr(path)?;
                let dst = self.reg()?;
                self.emit(
                    Instruction::Include {
                        dst,
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
            E::Print(expression) => {
                let register = self.expr(expression)?;
                self.emit(Instruction::Echo(register), line);
                self.literal(Constant::Int(1), line)
            }
            E::Exit(expression) => {
                let register = if let Some(expression) = expression {
                    self.expr(expression)?
                } else {
                    self.literal(Constant::Int(0), line)?
                };
                self.emit(Instruction::Exit(register), line);
                Ok(register)
            }
            E::Name(name) => {
                let (name, fallback) = self.resolve(name, 2);
                let dst = self.reg()?;
                self.emit(
                    Instruction::Constant {
                        dst,
                        name,
                        fallback,
                    },
                    line,
                );
                Ok(dst)
            }
            E::Call { callee, arguments } => self.compile_call(callee, arguments, line),
            E::Array(items) => self.compile_array(items, line),
            E::InterpolatedString(parts) => self.compile_interpolation(parts, line),
            E::Empty(value) => {
                let src = self.quiet(value)?;
                let dst = self.reg()?;
                self.emit(
                    Instruction::Unary {
                        dst,
                        operator: Unary::Not,
                        src,
                    },
                    line,
                );
                Ok(dst)
            }
            E::Isset(values) => {
                let dst = self.literal(Constant::Bool(false), line)?;
                let mut ends = vec![];
                for value in values {
                    let register = self.quiet(value)?;
                    ends.push(self.emit(
                        Instruction::BranchNull {
                            value: register,
                            target: Target(0),
                        },
                        line,
                    ));
                }
                let src = self.literal(Constant::Bool(true), line)?;
                self.emit(Instruction::Move { dst, src }, line);
                for jump in ends {
                    self.patch(jump, self.pos())
                }
                Ok(dst)
            }
            E::Match(match_expression) => self.compile_match(match_expression, line),
            E::MagicConstant(match_expression) => self.literal(
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
                        Constant::Bytes(if self.fun.name == "{main}" {
                            vec![]
                        } else {
                            self.fun.name.as_bytes().to_vec()
                        })
                    }
                    ast::MagicConstant::Namespace => {
                        Constant::Bytes(self.namespace.as_bytes().to_vec())
                    }
                    _ => Constant::Bytes(vec![]),
                },
                line,
            ),
            _ => Err(self.error("Expression not supported yet", line)),
        }
    }
}
