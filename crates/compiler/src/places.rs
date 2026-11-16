//! Destructuring, writable paths, and quiet reads.
use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Constant, Instruction, Place};
use crate::{CompileError, Compiler};
impl Compiler<'_> {
    pub(super) fn assign(
        &mut self,
        target: &ast::Expression,
        src: Reg,
        line: usize,
    ) -> Result<(), CompileError> {
        if let ast::ExpressionKind::List(items) = &target.kind {
            for (i, item) in items.iter().enumerate() {
                if let Some(item) = item {
                    let key = if let Some(key) = &item.key {
                        self.expr(key)?
                    } else {
                        self.literal(Constant::Int(i as i64), line)?
                    };
                    let dst = self.reg()?;
                    let place = Place {
                        base: src,
                        dynamic: false,
                        path: vec![Some(key)],
                    };
                    if item.by_reference {
                        self.emit(Instruction::Reference { dst, place }, line);
                        let target = self.place(&item.value)?;
                        self.emit(
                            Instruction::Rebind {
                                place: target,
                                src: dst,
                            },
                            line,
                        );
                    } else {
                        self.emit(
                            Instruction::Read {
                                dst,
                                place,
                                quiet: false,
                            },
                            line,
                        );
                        self.assign(&item.value, dst, line)?;
                    }
                }
            }
        } else {
            let place = self.place(target)?;
            self.emit(Instruction::Write { place, src }, line);
        }
        Ok(())
    }
    pub(super) fn quiet(&mut self, expression: &ast::Expression) -> Result<Reg, CompileError> {
        if matches!(
            expression.kind,
            ast::ExpressionKind::Variable(_) | ast::ExpressionKind::Index { .. }
        ) {
            let place = self.readable_place(expression)?;
            let dst = self.reg()?;
            self.emit(
                Instruction::Read {
                    dst,
                    place,
                    quiet: true,
                },
                self.lines.line(expression.span.start),
            );
            Ok(dst)
        } else {
            self.expr(expression)
        }
    }
}
