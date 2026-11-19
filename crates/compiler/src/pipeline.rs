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
use crate::{CompileError, passes::lower};

/// Included files return 1 by default; evaluated expressions return null.
#[derive(Clone, Copy)]
pub(super) enum SourceKind {
    File,
    CliFile,
    Eval { anonymous_seed: u64 },
}

/// Parse and lower on the dedicated parser stack, returning immutable bytecode.
pub(super) fn compile_on_stack(
    source: &[u8],
    file: &str,
    kind: SourceKind,
    native_functions: Option<&crate::NativeFunctions<'_>>,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    let lines = LineTable::from_bytes(source);
    let (parsed, mut scanner_diagnostics) =
        phera_parser::parse_file_on_current_stack(source, matches!(kind, SourceKind::CliFile));
    let compile_diagnostics = RefCell::new(Vec::new());
    let result = compile_program(
        parsed,
        source,
        file,
        &lines,
        kind,
        &compile_diagnostics,
        native_functions,
    );
    scanner_diagnostics.extend(compile_diagnostics.into_inner());
    match result {
        Ok(mut unit) => {
            unit.scanner_diagnostics = scanner_diagnostics;
            if !unit.classes.is_empty()
                || !unit.function_metadata.is_empty()
                || !unit.constant_metadata.is_empty()
            {
                unit.reflection_lines = Some(lines);
            }
            Ok(unit)
        }
        Err(mut error) => {
            let mut diagnostics = scanner_diagnostics;
            diagnostics.append(&mut error.scanner_diagnostics);
            error.scanner_diagnostics = diagnostics;
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
    native_functions: Option<&crate::NativeFunctions<'_>>,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    let ast = parsed.map_err(|error| {
        if error.is_compile_error() {
            return CompileError {
                message: error.message,
                file: file.into(),
                line: lines.line(error.span.start),
                parse: false,
                scanner_diagnostics: Vec::new(),
            };
        }
        let (line, message) = error.php_diagnostic(source, lines);
        let message = match kind {
            SourceKind::File | SourceKind::CliFile => message,
            SourceKind::Eval { .. } => {
                let display = String::from_utf8_lossy(source);
                let token = String::from_utf8_lossy(
                    source
                        .get(error.span.start..error.span.end)
                        .unwrap_or_default(),
                );
                crate::eval_diagnostics::message(
                    message,
                    &display,
                    &token,
                    error.span.start,
                    line,
                    lines,
                )
            }
        };
        CompileError {
            message,
            file: file.into(),
            line,
            parse: true,
            scanner_diagnostics: Vec::new(),
        }
    })?;
    let reflection = crate::context::ReflectionInputs::default();
    let mut compiler = Compiler::new(
        file,
        source,
        lines,
        "{main}",
        "",
        compile_diagnostics,
        &reflection,
    );
    compiler.native_functions = native_functions;
    if let SourceKind::Eval { anonymous_seed } = kind {
        compiler.anonymous_classes.set(anonymous_seed);
    }
    compiler.function.dynamic_scope = true;
    if let Err(mut error) = compiler
        .block(&ast.statements, DeclarationTiming::Hoisted)
        .and_then(|()| compiler.finish_gotos())
    {
        error
            .scanner_diagnostics
            .extend(compiler.declaration_warnings.into_iter().map(|warning| {
                phera_source::CompileDiagnostic {
                    kind: phera_source::CompileDiagnosticKind::Warning,
                    message: warning.message,
                    line: warning.line as usize,
                }
            }));
        return Err(error);
    }
    let default = match kind {
        SourceKind::File | SourceKind::CliFile => Constant::Int(1),
        SourceKind::Eval { .. } => Constant::Null,
    };
    let register = compiler.literal(default, 1)?;
    compiler.emit(Instruction::Return(register), 1);
    // The tree is not needed for lowering; a generated file's tree can be
    // hundreds of megabytes, so it goes before the bytecode tables are built.
    drop(ast);
    finish(compiler, source)
}

pub(super) fn finish(
    mut compiler: Compiler<'_>,
    source: &[u8],
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    let reflection_initializers = compiler.reflection.initializers.take();
    let keep_source =
        !reflection_initializers.is_empty() || compiler.reflection.has_doc_comments.get();
    let unit = CompiledUnit {
        main: compiler.finish()?,
        reflection_source: keep_source.then(|| source.into()),
        reflection_initializers,
        constant_metadata: compiler.constant_metadata,
        has_link_attributes: compiler.reflection.has_link_attributes.get(),
        declaration_warnings: compiler.declaration_warnings,
        functions: compiler.functions,
        hoisted: compiler.hoisted,
        classes: compiler.classes,
        hoisted_classes: compiler.hoisted_classes,
        file: compiler.file.into(),
    };
    validate_global_writes(&unit)?;
    Ok(lower::lower(unit, compiler.native_functions))
}

/// PHP 8.1 makes the global table read-only except through an indexed symbol name.
fn validate_global_writes(unit: &CompiledUnit) -> Result<(), CompileError> {
    use crate::ir::PlaceScope;
    for function in std::iter::once(&unit.main).chain(&unit.functions) {
        for (index, instruction) in function.instructions.iter().enumerate() {
            let place = match instruction {
                Instruction::Write { place, .. }
                | Instruction::WriteFromLocal { place, .. }
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
