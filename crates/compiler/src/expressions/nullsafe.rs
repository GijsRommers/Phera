//! Nullsafe chains branch before evaluating subsequent names, indices, or arguments.

use crate::ir::PathStep as Step;
use crate::{
    CompileError, Compiler,
    ir::{Constant, Instruction, Place, PlaceScope, Target},
};
use phera_ast as ast;
use phera_bytecode::{FetchMode, Reg};

/// Only postfix receivers propagate nullsafe short-circuiting to their enclosing chain.
pub(crate) fn contains_nullsafe(mut expression: &ast::Expression) -> bool {
    loop {
        match &expression.kind {
            ast::ExpressionKind::Property { nullsafe: true, .. }
            | ast::ExpressionKind::MethodCall { nullsafe: true, .. } => return true,
            ast::ExpressionKind::Property { target, .. }
            | ast::ExpressionKind::MethodCall { target, .. }
            | ast::ExpressionKind::Index { target, .. } => expression = target,
            _ => return false,
        }
    }
}

impl Compiler<'_> {
    /// Each chain has one null result and one shared exit after all ordinary operations.
    pub(crate) fn compile_nullsafe_chain(
        &mut self,
        expression: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let mut chain = Vec::new();
        let mut root = expression;
        while let ast::ExpressionKind::Property { target, .. }
        | ast::ExpressionKind::MethodCall { target, .. }
        | ast::ExpressionKind::Index { target, .. } = &root.kind
        {
            self.note_hook_backing(root);
            chain.push(root);
            root = target;
        }
        if chain.iter().any(|expression| {
            matches!(
                &expression.kind,
                ast::ExpressionKind::MethodCall {
                    arguments: ast::Arguments::FirstClassCallable,
                    ..
                }
            )
        }) {
            return Err(self.error(
                "Cannot combine nullsafe operator with Closure creation",
                line,
            ));
        }
        let destination = self.literal(Constant::Null, line)?;
        let mut current = self.compile_expression(root)?;
        let mut exits = Vec::new();
        for access in chain.into_iter().rev() {
            if matches!(
                access.kind,
                ast::ExpressionKind::Property { nullsafe: true, .. }
                    | ast::ExpressionKind::MethodCall { nullsafe: true, .. }
            ) {
                exits.push(self.emit(
                    Instruction::BranchNull {
                        value: current,
                        target: Target(0),
                    },
                    line,
                ));
            }
            if let ast::ExpressionKind::MethodCall {
                name: ast::MemberName::Identifier(name),
                arguments,
                ..
            } = &access.kind
            {
                self.emit(
                    Instruction::MethodCall {
                        receiver: current,
                        method: name.name.clone(),
                    },
                    line,
                );
                current = self.compile_arguments(arguments, line)?;
                continue;
            }
            let step = match &access.kind {
                ast::ExpressionKind::Property { name, .. } => {
                    let name = match name {
                        ast::MemberName::Identifier(name) => {
                            self.literal(Constant::Bytes(name.name.as_bytes().to_vec()), line)?
                        }
                        ast::MemberName::Expression(name) => self.compile_expression(name)?,
                    };
                    Step::DynamicProperty(name)
                }
                ast::ExpressionKind::Index {
                    index: Some(index), ..
                } => Step::Index(self.compile_expression(index)?),
                _ => return Err(self.error("Invalid nullsafe chain", line)),
            };
            let next = self.allocate_register()?;
            self.emit(
                Instruction::Read {
                    destination: next,
                    place: Place {
                        base: current,
                        scope: PlaceScope::Local,
                        path: vec![step],
                    },
                    mode: FetchMode::Read,
                },
                line,
            );
            current = next;
        }
        self.emit(
            Instruction::Move {
                destination,
                source: current,
            },
            line,
        );
        let end = self.function.instructions.len();
        for branch in exits {
            self.patch(branch, end);
        }
        Ok(destination)
    }
}
