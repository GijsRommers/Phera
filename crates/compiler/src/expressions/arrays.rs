//! Array construction and match/interpolation expression lowering.

use phera_ast as ast;
use phera_bytecode::Reg;
use phera_source::{CompileDiagnostic, CompileDiagnosticKind};

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
            let local_key = item
                .key
                .as_ref()
                .map(|key| self.compiled_variable(key))
                .transpose()?
                .flatten();
            let key = match (&item.key, local_key) {
                (_, Some(local)) => Some(local),
                (Some(key), None) => Some(self.compile_expression(key)?),
                (None, None) => None,
            };
            let value = if item.by_reference {
                let place = self.reference_source(&item.value, line)?;
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
                if local_key.is_some() {
                    self.operand_line(&item.value)
                } else {
                    line
                },
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
        // Zend starts with the list's first part and reports the notice
        // before compiling the current part updates its source line.
        let mut diagnostic_line = parts
            .first()
            .map_or(line, |part| self.interpolation_line(part));
        for part in parts {
            let mark = self.mark_temporaries();
            let rhs = match part {
                ast::StringPart::Literal(value, _) => {
                    self.literal(Constant::Bytes(value.clone()), line)?
                }
                ast::StringPart::Expression(expression) => self.compile_expression(expression)?,
                ast::StringPart::DollarCurly(expression, form) => {
                    let message = match form {
                        ast::DollarCurlyForm::Name => {
                            "Using ${var} in strings is deprecated, use {$var} instead"
                        }
                        ast::DollarCurlyForm::Expression => {
                            "Using ${expr} (variable variables) in strings is deprecated, use {${expr}} instead"
                        }
                    };
                    self.compile_diagnostics
                        .borrow_mut()
                        .push(CompileDiagnostic {
                            kind: CompileDiagnosticKind::Deprecated,
                            message: message.into(),
                            line: diagnostic_line,
                        });
                    self.compile_expression(expression)?
                }
            };
            diagnostic_line = self.interpolation_line(part);
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

    fn interpolation_line(&self, part: &ast::StringPart) -> usize {
        let span = match part {
            ast::StringPart::Literal(_, span) => *span,
            ast::StringPart::Expression(expression)
            | ast::StringPart::DollarCurly(expression, _) => expression.span,
        };
        self.lines.line(span.start)
    }
}
