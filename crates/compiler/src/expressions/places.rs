//! Destructuring, writable paths, and quiet reads.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Constant, Instruction, Place};
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Destructure in item order, binding references only for reference list elements.
    pub(crate) fn assign(
        &mut self,
        target: &ast::Expression,
        src: Reg,
        line: usize,
    ) -> Result<(), CompileError> {
        if let ast::ExpressionKind::List(items) = &target.kind {
            for (i, item) in items.iter().enumerate() {
                if let Some(item) = item {
                    let key = if let Some(key) = &item.key {
                        self.compile_expression(key)?
                    } else {
                        self.literal(Constant::Int(i as i64), line)?
                    };
                    let dst = self.allocate_register()?;
                    let place = Place {
                        base: src,
                        scope: crate::ir::PlaceScope::Local,
                        path: vec![crate::ir::PathStep::Index(key)],
                    };
                    if item.by_reference || contains_reference(&item.value) {
                        self.emit(
                            Instruction::Reference {
                                destination: dst,
                                place,
                            },
                            line,
                        );
                        if !item.by_reference {
                            self.assign(&item.value, dst, line)?;
                            self.emit(Instruction::Clear { register: dst }, line);
                            continue;
                        }
                        let target = self.place(&item.value)?;
                        self.emit(
                            Instruction::Rebind {
                                place: target,
                                source: dst,
                            },
                            line,
                        );
                    } else {
                        self.emit(
                            Instruction::Read {
                                destination: dst,
                                place,
                                mode: phera_bytecode::FetchMode::Destructure,
                            },
                            line,
                        );
                        self.assign(&item.value, dst, line)?;
                    }
                }
            }
        } else {
            let place = self.place(target)?;
            self.emit(Instruction::Write { place, source: src }, line);
        }
        Ok(())
    }

    /// Read variables and indexes without undefined-value warnings; evaluate other expressions normally.
    pub(crate) fn quiet(&mut self, expression: &ast::Expression) -> Result<Reg, CompileError> {
        if matches!(
            expression.kind,
            ast::ExpressionKind::Variable(_)
                | ast::ExpressionKind::Index { .. }
                | ast::ExpressionKind::Property { .. }
                | ast::ExpressionKind::StaticProperty { .. }
        ) {
            let place = self.readable_place(expression)?;
            let dst = self.allocate_register()?;
            self.emit(
                Instruction::Read {
                    destination: dst,
                    place,
                    mode: phera_bytecode::FetchMode::Quiet,
                },
                self.lines.line(expression.span.start),
            );
            Ok(dst)
        } else {
            self.compile_expression(expression)
        }
    }
}

/// A nested reference requires every ancestor array to retain its original writable cell.
pub(crate) fn contains_reference(expression: &ast::Expression) -> bool {
    let ast::ExpressionKind::List(items) = &expression.kind else {
        return false;
    };
    items
        .iter()
        .flatten()
        .any(|item| item.by_reference || contains_reference(&item.value))
}
