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
        if nullsafe::contains_nullsafe(expression) {
            return self.compile_nullsafe_chain(expression, line);
        }
        if let Some(condition) = constant(expression) {
            return self.literal(condition, line);
        }
        match &expression.kind {
            ExpressionKind::Yield { .. } | ExpressionKind::YieldFrom(_) => {
                Err(self.error("Generators not supported yet", line))
            }
            ExpressionKind::ArrowFunction(arrow) => {
                self.compile_arrow(arrow, expression.span, line)
            }
            ExpressionKind::Closure(closure) => {
                self.compile_closure(closure, expression.span, line)
            }
            ExpressionKind::Throw(operand) => {
                let source = self.compile_expression(operand)?;
                self.emit(crate::ir::Instruction::Throw { source }, line);
                self.allocate_register()
            }
            ExpressionKind::ClassConstant { .. }
            | ExpressionKind::New { .. }
            | ExpressionKind::MethodCall { .. }
            | ExpressionKind::StaticCall { .. }
            | ExpressionKind::Clone { .. }
            | ExpressionKind::InstanceOf { .. } => self.compile_object_expression(expression, line),
            ExpressionKind::Variable(ast::Variable::Named(name)) if name == "this" => {
                let destination = self.allocate_register()?;
                self.emit(crate::ir::Instruction::This { destination }, line);
                Ok(destination)
            }
            ExpressionKind::Variable(_)
            | ExpressionKind::Index { .. }
            | ExpressionKind::Property { .. }
            | ast::ExpressionKind::StaticProperty { .. } => self.compile_read(expression, line),
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
            ExpressionKind::Eval(code) => self.compile_eval(code, line),
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
        if *match_expression == ast::MagicConstant::Class && self.trait_scope.is_some() {
            let destination = self.allocate_register()?;
            self.emit(
                crate::ir::Instruction::ClassName {
                    destination,
                    class: phera_bytecode::TRAIT_CLASS_NAME.into(),
                },
                line,
            );
            return Ok(destination);
        }
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
                    } else if *match_expression == ast::MagicConstant::Function
                        && let Some((_, method)) = self.function.name.split_once("::")
                        && !self.function.name.starts_with("{closure:")
                    {
                        method.as_bytes().to_vec()
                    } else {
                        self.function.name.as_bytes().to_vec()
                    })
                }
                ast::MagicConstant::Class => Constant::Bytes(
                    self.class_scope
                        .as_deref()
                        .unwrap_or("")
                        .as_bytes()
                        .to_vec(),
                ),
                ast::MagicConstant::Namespace => {
                    Constant::Bytes(self.namespace.as_bytes().to_vec())
                }
                ast::MagicConstant::Trait => Constant::Bytes(
                    self.trait_scope
                        .as_deref()
                        .unwrap_or("")
                        .as_bytes()
                        .to_vec(),
                ),
                ast::MagicConstant::Property => Constant::Bytes(
                    self.hooked_property
                        .as_deref()
                        .unwrap_or("")
                        .as_bytes()
                        .to_vec(),
                ),
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
pub(crate) mod nullsafe;
pub(crate) mod objects;
pub(crate) mod operators;
mod parent_hooks;
pub(crate) mod paths;
pub(crate) mod places;
pub(crate) mod spine;
pub(crate) mod values;

pub(crate) mod closures;

pub(crate) mod arrows;
