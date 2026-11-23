//! A literal `sprintf('%s: %d', $label, $number)` uses ordinary casts and concatenation.

use phera_ast::{Argument, ExpressionKind, Literal};
use phera_bytecode::Reg;

use crate::ir::{Binary, Constant, Instruction, Unary};
use crate::{CompileError, Compiler};

enum Part {
    Bytes(Vec<u8>),
    String,
    Int,
}

impl Compiler<'_> {
    pub(super) fn compile_literal_sprintf(
        &mut self,
        arguments: &[Argument],
        line: usize,
    ) -> Result<Option<Reg>, CompileError> {
        let Some((format, values)) = arguments.split_first() else {
            return Ok(None);
        };
        let ExpressionKind::Literal(Literal::String(bytes)) = &format.value.kind else {
            return Ok(None);
        };
        let Some(parts) = format_parts(bytes, values.len()) else {
            return Ok(None);
        };
        // Argument expressions can change a local or throw before the casts.
        let values = values
            .iter()
            .map(|argument| self.builtin_operand(&argument.value, line))
            .collect::<Result<Vec<_>, _>>()?;
        let mut values = values.into_iter();
        let mut result = None;
        for part in parts {
            let piece = match part {
                Part::Bytes(bytes) => self.literal(Constant::Bytes(bytes), line)?,
                Part::String | Part::Int => {
                    let mut source = values
                        .next()
                        .ok_or_else(|| self.error("Invalid format lowering", line))?;
                    if matches!(part, Part::Int) {
                        source = self.format_cast(Unary::Int, source, line)?;
                    }
                    self.format_cast(Unary::String, source, line)?
                }
            };
            result = Some(if let Some(left) = result {
                let destination = self.allocate_register()?;
                self.emit(
                    Instruction::Binary {
                        destination,
                        operator: Binary::Concat,
                        left,
                        right: piece,
                    },
                    line,
                );
                destination
            } else {
                piece
            });
        }
        match result {
            Some(result) => Ok(Some(result)),
            None => self.literal(Constant::Bytes(Vec::new()), line).map(Some),
        }
    }

    fn format_cast(
        &mut self,
        operator: Unary,
        source: Reg,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let destination = self.allocate_register()?;
        self.emit(
            Instruction::Unary {
                destination,
                operator,
                source,
            },
            line,
        );
        Ok(destination)
    }
}

/// Whether `sprintf` with this literal format and `supplied` values is
/// compiled into a concatenation (see [`format_parts`]).
pub(super) fn is_compiled_format(bytes: &[u8], supplied: usize) -> bool {
    format_parts(bytes, supplied).is_some()
}

/// Zend accepts only simple %s and %d placeholders and literal %% escapes.
fn format_parts(bytes: &[u8], supplied: usize) -> Option<Vec<Part>> {
    if bytes.len() >= 256 {
        return None;
    }
    let mut bytes = bytes.iter().copied();
    let mut parts = Vec::new();
    let mut text = Vec::new();
    let mut count = 0;
    while let Some(byte) = bytes.next() {
        if byte != b'%' {
            text.push(byte);
            continue;
        }
        let part = match bytes.next()? {
            b'%' => {
                text.push(b'%');
                continue;
            }
            b's' => Part::String,
            b'd' => Part::Int,
            _ => return None,
        };
        if !text.is_empty() {
            parts.push(Part::Bytes(std::mem::take(&mut text)));
        }
        parts.push(part);
        count += 1;
    }
    if !text.is_empty() {
        parts.push(Part::Bytes(text));
    }
    (count == supplied).then_some(parts)
}
