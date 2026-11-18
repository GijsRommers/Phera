//! Array construction and match/interpolation expression lowering.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Binary, Constant, Instruction};
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Evaluate each key before its value, retaining explicit reference elements.
    pub(crate) fn compile_array(
        &mut self,
        items: &[ast::ArrayItem],
        line: usize,
    ) -> Result<Reg, CompileError> {
        let dst = self.allocate_register()?;
        self.emit(Instruction::Array(dst), line);
        for item in items {
            let mark = self.mark_temporaries();
            let key = item
                .key
                .as_ref()
                .map(|key| self.compile_expression(key))
                .transpose()?;
            let value = if item.by_reference {
                let place = self.place(&item.value)?;
                let dst = self.allocate_register()?;
                self.emit(
                    Instruction::Reference {
                        destination: dst,
                        place,
                    },
                    line,
                );
                dst
            } else {
                self.compile_expression(&item.value)?
            };
            self.emit(
                Instruction::ArrayPush {
                    array: dst,
                    key,
                    value,
                    spread: item.spread,
                },
                line,
            );
            self.release_temporaries(mark);
        }
        Ok(dst)
    }
}

impl Compiler<'_> {
    /// Concatenate literal and expression parts in source order.
    pub(crate) fn compile_interpolation(
        &mut self,
        parts: &[ast::StringPart],
        line: usize,
    ) -> Result<Reg, CompileError> {
        // One register accumulates the string; each part's temporaries are
        // handed back once it has been appended.
        let dst = self.literal(Constant::Bytes(vec![]), line)?;
        for part in parts {
            let mark = self.mark_temporaries();
            let rhs = match part {
                ast::StringPart::Literal(value) => {
                    self.literal(Constant::Bytes(value.clone()), line)?
                }
                ast::StringPart::Expression(expression) => self.compile_expression(expression)?,
            };
            self.emit(
                Instruction::Binary {
                    destination: dst,
                    operator: Binary::Concat,
                    left: dst,
                    right: rhs,
                },
                line,
            );
            self.release_temporaries(mark);
        }
        Ok(dst)
    }
}
