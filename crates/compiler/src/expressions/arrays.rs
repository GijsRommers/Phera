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
        let mut dst = self.literal(Constant::Bytes(vec![]), line)?;
        for part in parts {
            let rhs = match part {
                ast::StringPart::Literal(value) => {
                    self.literal(Constant::Bytes(value.clone()), line)?
                }
                ast::StringPart::Expression(expression) => self.compile_expression(expression)?,
            };
            let next = self.allocate_register()?;
            self.emit(
                Instruction::Binary {
                    destination: next,
                    operator: Binary::Concat,
                    left: dst,
                    right: rhs,
                },
                line,
            );
            dst = next;
        }
        Ok(dst)
    }
}
