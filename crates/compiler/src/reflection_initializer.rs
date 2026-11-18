//! `#[Column(new Options)]` compiles its validated argument on the first read.

use std::cell::RefCell;

use phera_ast::StatementKind;
use phera_bytecode::{CompiledUnit, reflection::InitializerSource};
use phera_source::LineTable;

use crate::{CompileError, Compiler, ir::Instruction};

/// Compile a fragment using the file and imports from its PHP declaration.
pub fn compile_reflection_initializer(
    source: &[u8],
    file: &str,
    initializer: &InitializerSource,
) -> Result<CompiledUnit, CompileError> {
    crate::on_compile_stack(file, || compile_on_stack(source, file, initializer))
}

fn compile_on_stack(
    source: &[u8],
    file: &str,
    initializer: &InitializerSource,
) -> Result<CompiledUnit, CompileError> {
    let error = || CompileError {
        message: "Invalid source for reflection initializer".into(),
        file: file.into(),
        line: initializer.first_line,
        parse: false,
        scanner_diagnostics: Vec::new(),
    };
    let expression = source
        .get(initializer.span.start..initializer.span.end)
        .ok_or_else(error)?;
    let fragment = [b"<?php return ", expression, b"\n;"].concat();
    let lines = LineTable::fragment(&fragment, initializer.first_line);
    let parsed =
        phera_parser::parse_bytes_on_current_stack(&fragment).map_err(|error| CompileError {
            message: error.message,
            file: file.into(),
            line: lines.line(error.span.start),
            parse: true,
            scanner_diagnostics: Vec::new(),
        })?;
    let [statement] = parsed.statements.as_slice() else {
        return Err(error());
    };
    let StatementKind::Return(Some(expression)) = &statement.kind else {
        return Err(error());
    };
    let diagnostics = RefCell::new(Vec::new());
    let nested_initializers = RefCell::new(Vec::new());
    let mut compiler = Compiler::new(
        file,
        &lines,
        &initializer.function,
        &initializer.namespace,
        &diagnostics,
        &nested_initializers,
    );
    compiler.source = &fragment;
    compiler.class_scope.clone_from(&initializer.scope);
    compiler.imports.clone_from(&initializer.imports);
    compiler.function.strict_types = initializer.strict_types;
    let result = compiler.compile_expression(expression)?;
    compiler.function.name = "{reflection initializer}".into();
    compiler.emit(Instruction::Return(result), initializer.first_line);
    crate::pipeline::finish(compiler, &fragment)
}
