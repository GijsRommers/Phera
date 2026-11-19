//! Long member and call chains, compiled bottom-up in a loop.
//!
//! ```php
//! <?php
//! $builder->add('a')->add('b')->add('c') /* ... 100,000 more ... */;
//! echo $matrix[0][0][0] /* ... */;
//! $f()()()();
//! ```
//!
//! Each rule of the compiler compiles its operand by calling
//! [`Compiler::compile_expression`] on it, so a chain of n calls would recurse
//! n deep. For a chain longer than [`LONG_CHAIN`] steps the compiler instead
//! walks down to the chain's start, compiles that, and then compiles each step
//! from the bottom up. Before a step is compiled, its operand is registered as
//! already compiled ([`CompiledOperand`]); when the step's rule asks for the
//! operand, it gets that register back at once instead of recursing.
//!
//! Only operands that a rule compiles first, as a plain value, are followed
//! (see [`chain_operand`]); `@` is not, because it emits code before its
//! operand. The steps' temporaries are handed back after each step, so the
//! chain's length does not use up registers: each result is moved into one
//! register that carries the chain's value up to the next step.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::expressions::nullsafe::contains_nullsafe;
use crate::ir::Instruction;
use crate::{CompileError, Compiler};

/// Chains up to this many steps are compiled by ordinary recursion, which
/// needs no extra moves. Longer ones take the loop.
pub(crate) const LONG_CHAIN: usize = 64;

/// The operand of the step being compiled, compiled already. The pointer is
/// only compared with the expression a rule asks to compile, never followed.
pub(crate) struct CompiledOperand {
    operand: *const ast::Expression,
    register: Reg,
}

impl Compiler<'_> {
    /// The register of `expression` if it is the operand of the chain step
    /// being compiled.
    pub(crate) fn take_compiled_operand(&mut self, expression: &ast::Expression) -> Option<Reg> {
        let compiled = self.compiled_operand.as_ref()?;
        if !std::ptr::eq(compiled.operand, expression) {
            return None;
        }
        self.compiled_operand
            .take()
            .map(|compiled| compiled.register)
    }

    /// Whether `expression` is the operand of the chain step being compiled.
    pub(crate) fn is_compiled_operand(&self, expression: &ast::Expression) -> bool {
        self.compiled_operand
            .as_ref()
            .is_some_and(|compiled| std::ptr::eq(compiled.operand, expression))
    }

    /// Compiles `expression` with the loop when it starts a long chain;
    /// `None` when it does not, so the caller compiles it the ordinary way.
    pub(crate) fn compile_long_chain(
        &mut self,
        expression: &ast::Expression,
    ) -> Option<Result<Reg, CompileError>> {
        let steps = chain_steps(expression, LONG_CHAIN);
        if steps.len() < LONG_CHAIN {
            return None;
        }
        Some(self.compile_chain_steps(chain_steps(expression, usize::MAX)))
    }

    fn compile_chain_steps(
        &mut self,
        steps: Vec<(&ast::Expression, &ast::Expression)>,
    ) -> Result<Reg, CompileError> {
        let Some(&(_, start)) = steps.last() else {
            return Err(self.error("Phera internal error: empty chain", 1));
        };
        let line = self.lines.line(start.span.start);
        let value = self.compile_expression(start)?;
        let carrier = self.allocate_register()?;
        self.emit(
            Instruction::Move {
                destination: carrier,
                source: value,
            },
            line,
        );
        let mark = self.mark_temporaries();
        for (step, operand) in steps.into_iter().rev() {
            let line = self.lines.line(step.span.start);
            self.compiled_operand = Some(CompiledOperand {
                operand,
                register: carrier,
            });
            let value = self.compile_expression_kind(step, line)?;
            if self.compiled_operand.take().is_some() {
                return Err(self.error("Phera internal error: chain operand not compiled", line));
            }
            if value != carrier {
                self.emit(
                    Instruction::Move {
                        destination: carrier,
                        source: value,
                    },
                    line,
                );
            }
            self.release_temporaries(mark);
        }
        Ok(carrier)
    }
}

/// The steps of the chain that `expression` ends, top first, each with its
/// operand; at most `limit` of them.
fn chain_steps(
    expression: &ast::Expression,
    limit: usize,
) -> Vec<(&ast::Expression, &ast::Expression)> {
    let mut steps = Vec::new();
    let mut node = expression;
    // Whether `node` continues a run of `->`, `[]` and calls already checked
    // for `?->`, which makes the whole run one nullsafe chain.
    let mut checked_run = false;
    while steps.len() < limit {
        let member = is_member_access(node);
        if member && !checked_run && contains_nullsafe(node) {
            break;
        }
        checked_run = member;
        let Some(operand) = chain_operand(node) else {
            break;
        };
        steps.push((node, operand));
        node = operand;
    }
    steps
}

fn is_member_access(expression: &ast::Expression) -> bool {
    matches!(
        expression.kind,
        ast::ExpressionKind::Property { .. }
            | ast::ExpressionKind::MethodCall { .. }
            | ast::ExpressionKind::Index { .. }
    )
}

/// The operand a chain continues through: one that the rule for `expression`
/// compiles first, as a value, with [`Compiler::compile_expression`].
fn chain_operand(expression: &ast::Expression) -> Option<&ast::Expression> {
    use ast::ExpressionKind;
    let not_a_name = |class: &ast::Expression| !matches!(class.kind, ExpressionKind::Name(_));
    match &expression.kind {
        ExpressionKind::MethodCall {
            target,
            nullsafe: false,
            ..
        } => Some(target),
        ExpressionKind::Call { callee, .. } if not_a_name(callee) => Some(callee),
        ExpressionKind::StaticCall { class, .. } | ExpressionKind::ClassConstant { class, .. }
            if not_a_name(class) =>
        {
            Some(class)
        }
        ExpressionKind::InstanceOf { value, .. } => Some(value),
        ExpressionKind::Ternary { condition, .. } => Some(condition),
        ExpressionKind::Unary { operator, operand }
            if *operator != ast::UnaryOperator::ErrorSuppress =>
        {
            Some(operand)
        }
        ExpressionKind::Cast { operand, .. } => Some(operand),
        ExpressionKind::Clone { object, with: None } => Some(object),
        // `$a[0]` reads its target as the base of an access path (see
        // `readable_place`), which stops at an operand compiled already. The
        // first step on a variable is where the chain starts.
        ExpressionKind::Index { target, .. } | ExpressionKind::Property { target, .. } => {
            match target.kind {
                ExpressionKind::Variable(_) | ExpressionKind::StaticProperty { .. } => None,
                _ => Some(target),
            }
        }
        _ => None,
    }
}
