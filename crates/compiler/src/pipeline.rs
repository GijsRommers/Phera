//! Parse source and lower AST on the dedicated compiler stack.
//!
//! The parser supplies an owned AST. Lowering preserves evaluation order and
//! produces IR with local variables before temporaries. For example, an echo
//! of an integer becomes a literal load followed by Echo. Final emission interns
//! names and paths and translates IR targets to bytecode offsets.

use phera_source::LineTable;

use crate::context::Compiler;
use crate::ir::{CompiledUnit, Constant, Instruction};
use crate::policies::DeclarationTiming;
use crate::{CompileError, errors, passes::lower};

/// Parse and lower on the dedicated parser stack, returning immutable bytecode.
pub(super) fn compile_on_stack(
    source: &str,
    file: &str,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    let lines = LineTable::new(source);
    let ast = phera_parser::parse_on_current_stack(source).map_err(|error| CompileError {
        message: errors::parse_message(
            &error.message,
            source,
            source
                .get(error.span.start..error.span.end)
                .unwrap_or_default(),
        ),
        file: file.into(),
        line: errors::parse_line(&error.message, source, lines.line(error.span.start)),
        parse: true,
    })?;
    let mut compiler = Compiler::new(file, &lines, "{main}", "");
    compiler.function.dynamic_scope = true;
    compiler.block(&ast.statements, DeclarationTiming::Hoisted)?;
    let register = compiler.literal(Constant::Int(1), 1)?;
    compiler.emit(Instruction::Return(register), 1);
    let unit = CompiledUnit {
        main: compiler.finish(),
        declaration_warnings: compiler.declaration_warnings,
        functions: compiler.functions,
        hoisted: compiler.hoisted,
        classes: compiler.classes,
        hoisted_classes: compiler.hoisted_classes,
        file: file.into(),
    };
    validate_global_writes(&unit)?;
    Ok(lower::lower(unit))
}

/// PHP 8.1 makes the global table read-only except through an indexed symbol name.
fn validate_global_writes(unit: &CompiledUnit) -> Result<(), CompileError> {
    use crate::ir::PlaceScope;
    for function in std::iter::once(&unit.main).chain(&unit.functions) {
        for (index, instruction) in function.instructions.iter().enumerate() {
            let place = match instruction {
                Instruction::Write { place, .. }
                | Instruction::Rebind { place, .. }
                | Instruction::Compound { place, .. }
                | Instruction::IncPlace { place, .. }
                | Instruction::Bind { place, .. }
                | Instruction::Unset(place)
                | Instruction::Reference { place, .. } => place,
                _ => continue,
            };
            if place.scope == PlaceScope::Globals {
                return Err(CompileError {
                    message:
                        "$GLOBALS can only be modified using the $GLOBALS[$name] = $value syntax"
                            .into(),
                    file: unit.file.clone(),
                    line: function.lines[index],
                    parse: false,
                });
            }
        }
    }
    Ok(())
}
