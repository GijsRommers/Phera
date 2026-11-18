//! `#[Column(new Options)]` compiles its validated argument on the first read.

use std::cell::RefCell;

use phera_ast::{ExpressionKind, MemberName, StatementKind};
use phera_bytecode::{CompiledUnit, reflection::InitializerSource};
use phera_source::LineTable;

use crate::{CompileError, Compiler, ir::Instruction};

pub struct CompiledInitializer {
    pub unit: CompiledUnit,
    /// A direct class constant read can reuse an immutable native value.
    pub class_constant: Option<(String, String)>,
}

/// Compile a fragment using the file and imports from its PHP declaration.
pub fn compile_reflection_initializer(
    source: &[u8],
    file: &str,
    initializer: &InitializerSource,
    in_trait: bool,
) -> Result<CompiledInitializer, CompileError> {
    crate::on_compile_stack(file, || {
        compile_on_stack(source, file, initializer, in_trait)
    })
}

fn compile_on_stack(
    source: &[u8],
    file: &str,
    initializer: &InitializerSource,
    in_trait: bool,
) -> Result<CompiledInitializer, CompileError> {
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
    let reflection = crate::context::ReflectionInputs::default();
    let mut compiler = Compiler::new(
        file,
        &fragment,
        &lines,
        &initializer.function,
        &initializer.namespace,
        &diagnostics,
        &reflection,
    );
    compiler.class_scope.clone_from(&initializer.scope);
    if in_trait {
        compiler.trait_scope.clone_from(&initializer.scope);
    }
    compiler.imports.clone_from(&initializer.imports);
    compiler.function.strict_types = initializer.strict_types;
    let class_constant = match &expression.kind {
        ExpressionKind::ClassConstant {
            class,
            name: MemberName::Identifier(name),
        } if !name.name.eq_ignore_ascii_case("class") => compiler
            .object_class_name(class, initializer.first_line)
            .ok()
            .filter(|name| !matches!(name.as_str(), "self" | "parent" | "static"))
            .map(|class| (class, name.name.clone())),
        _ => None,
    };
    let result = compiler.compile_expression(expression)?;
    compiler.function.name = "{reflection initializer}".into();
    compiler.emit(Instruction::Return(result), initializer.first_line);
    let mut unit = crate::pipeline::finish(compiler, &fragment)?;
    unit.reflection_lines = Some(lines);
    Ok(CompiledInitializer {
        unit,
        class_constant,
    })
}
