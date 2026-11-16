//! AST lowering on the parser's dedicated stack. Long expression spines use a work list.
mod context;
use context::{Compiler, Loop};
mod functions;
mod helpers;
use helpers::{binary, constant, type_name, writable};
mod arrays;
mod calls;
mod direct_operands;
mod errors;
mod expression_spine;
mod expressions;
mod integer_operands;
mod ir;
mod lower;
mod lowering;
mod lowering_tables;
mod matching;
mod optimize;
mod places;
mod statements;
mod switching;
use ir::{CompiledUnit, Constant, Instruction};
use phera_source::LineTable;
mod compile_error;
pub use compile_error::CompileError;

/// Compile PHP source on a bounded dedicated stack, keeping tiny callers safe.
pub fn compile(source: &str, file: &str) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("phera-compiler".into())
            .stack_size(phera_parser::PARSER_STACK_BYTES)
            .spawn_scoped(scope, || compile_on_stack(source, file))
            .map_err(|error| CompileError {
                message: error.to_string(),
                file: file.into(),
                line: 1,
                parse: false,
            })?
            .join()
            .unwrap_or_else(|_| {
                Err(CompileError {
                    message: "Phera internal error: compiler thread failed".into(),
                    file: file.into(),
                    line: 1,
                    parse: false,
                })
            })
    })
}

#[cfg(test)]
mod tests;

/// Parse and lower on the dedicated parser stack, returning immutable bytecode.
fn compile_on_stack(
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
    compiler.fun.dynamic_scope = true;
    compiler.block(&ast.statements, true)?;
    let register = compiler.literal(Constant::Int(1), 1)?;
    compiler.emit(Instruction::Return(register), 1);
    Ok(lower::lower(CompiledUnit {
        main: compiler.finish(),
        functions: compiler.functions,
        hoisted: compiler.hoisted,
        file: file.into(),
    }))
}
