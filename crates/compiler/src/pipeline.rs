//! Parse source and lower AST on the dedicated compiler stack.
//!
//! The parser supplies an owned AST. Lowering preserves evaluation order and
//! produces IR with local variables before temporaries. For example, an echo
//! of an integer becomes a literal load followed by Echo. Final emission interns
//! names and paths and translates IR targets to bytecode offsets.

use std::cell::RefCell;

use phera_source::{CompileDiagnostic, LineTable};

use crate::context::Compiler;
use crate::ir::{CompiledUnit, Constant, Instruction};
use crate::policies::DeclarationTiming;
use crate::{CompileError, errors, passes::lower};

/// Included files return 1 by default; evaluated expressions return null.
#[derive(Clone, Copy)]
pub(super) enum SourceKind {
    File,
    Eval,
}

/// Parse and lower on the dedicated parser stack, returning immutable bytecode.
pub(super) fn compile_on_stack(
    source: &[u8],
    file: &str,
    kind: SourceKind,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    let lines = LineTable::from_bytes(source);
    let (parsed, mut scanner_diagnostics) = phera_parser::parse_file_on_current_stack(source);
    let compile_diagnostics = RefCell::new(Vec::new());
    let result = compile_program(parsed, source, file, &lines, kind, &compile_diagnostics);
    scanner_diagnostics.extend(compile_diagnostics.into_inner());
    match result {
        Ok(mut unit) => {
            unit.scanner_diagnostics = scanner_diagnostics;
            Ok(unit)
        }
        Err(mut error) => {
            error.scanner_diagnostics = scanner_diagnostics;
            Err(error)
        }
    }
}

fn compile_program(
    parsed: Result<phera_ast::Program, phera_parser::ParseError>,
    source: &[u8],
    file: &str,
    lines: &LineTable,
    kind: SourceKind,
    compile_diagnostics: &RefCell<Vec<CompileDiagnostic>>,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    let ast = parsed.map_err(|error| {
        if error.is_backtick_operator() {
            return CompileError {
                message: error.message,
                file: file.into(),
                line: lines.line(error.span.start),
                parse: false,
                scanner_diagnostics: Vec::new(),
            };
        }
        let display = String::from_utf8_lossy(source);
        let token = source
            .get(error.span.start..error.span.end)
            .unwrap_or_default();
        let token = String::from_utf8_lossy(token);
        let line = errors::parse_line(&error.message, &display, lines.line(error.span.start));
        let message = errors::parse_message(&error.message, &display, &token, line);
        let message = match kind {
            SourceKind::File => message,
            SourceKind::Eval => crate::eval_diagnostics::message(
                message,
                &display,
                &token,
                error.span.start,
                line,
                lines,
            ),
        };
        CompileError {
            message,
            file: file.into(),
            line,
            parse: true,
            scanner_diagnostics: Vec::new(),
        }
    })?;
    let mut compiler = Compiler::new(file, lines, "{main}", "", compile_diagnostics);
    compiler.function.dynamic_scope = true;
    compiler.block(&ast.statements, DeclarationTiming::Hoisted)?;
    let default = match kind {
        SourceKind::File => Constant::Int(1),
        SourceKind::Eval => Constant::Null,
    };
    let register = compiler.literal(default, 1)?;
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
    // The tree is not needed for lowering; a generated file's tree can be
    // hundreds of megabytes, so it goes before the bytecode tables are built.
    drop(ast);
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
                    scanner_diagnostics: Vec::new(),
                });
            }
        }
    }
    Ok(())
}
