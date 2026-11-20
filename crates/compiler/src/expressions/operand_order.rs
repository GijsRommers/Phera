//! Zend consumes a compiled variable at its use, as in `$a + ($a = 5)`.

use phera_ast::{Arguments, Expression, ExpressionKind as E, Variable};
use phera_bytecode::Reg;

use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Superglobals, `$this`, and computed accesses still need their own fetch.
    pub(super) fn compiled_variable(
        &mut self,
        expression: &Expression,
    ) -> Result<Option<Reg>, CompileError> {
        match &expression.kind {
            E::Variable(Variable::Named(name))
                if name != "this" && name != "GLOBALS" && !is_superglobal(name) =>
            {
                self.local(name).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// Use the line left by evaluating the operand, including the last
    /// argument of a multiline call.
    pub(super) fn operand_line(&self, mut expression: &Expression) -> usize {
        loop {
            expression = match &expression.kind {
                E::Binary { right, .. } | E::CompoundAssign { value: right, .. } => right,
                E::Assign { target, .. } | E::AssignReference { target, .. } => target,
                E::Cast { operand, .. } | E::Unary { operand, .. } => operand,
                E::Ternary { otherwise, .. } => otherwise,
                E::Call { arguments, .. }
                | E::MethodCall { arguments, .. }
                | E::StaticCall { arguments, .. }
                | E::New { arguments, .. }
                    if let Arguments::List(arguments) = arguments
                        && let Some(argument) = arguments.last() =>
                {
                    &argument.value
                }
                _ => return self.lines.line(expression.span.start),
            };
        }
    }
}

pub(super) fn is_superglobal(name: &str) -> bool {
    matches!(
        name,
        "_GET" | "_POST" | "_COOKIE" | "_SERVER" | "_REQUEST" | "_FILES" | "_ENV" | "_SESSION"
    )
}
