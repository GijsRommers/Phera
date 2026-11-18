//! Compile PHP source into immutable register bytecode on a dedicated stack.

mod compile_error;
mod constant_callables;
mod context;
mod errors;
mod eval_diagnostics;
mod expressions;
mod helpers;
mod ir;
mod parameter_defaults;
mod passes;
mod pipeline;
mod policies;
mod statements;

#[cfg(test)]
mod tests;

pub use compile_error::CompileError;

use context::{Compiler, Loop};
use helpers::{binary, constant, type_name, writable};

/// Compile PHP source on a bounded dedicated stack, keeping tiny callers safe.
pub fn compile(source: &str, file: &str) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    compile_bytes(source.as_bytes(), file)
}

/// Compile a file without transcoding its source bytes.
pub fn compile_bytes(
    source: &[u8],
    file: &str,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    compile_source(source, file, pipeline::SourceKind::File)
}

/// Compile the code passed to `eval()` without requiring an opening tag.
pub fn compile_eval(
    source: &[u8],
    file: &str,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    let code = [b"<?php ".as_slice(), source].concat();
    compile_source(&code, file, pipeline::SourceKind::Eval)
}

fn compile_source(
    source: &[u8],
    file: &str,
    kind: pipeline::SourceKind,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("phera-compiler".into())
            .stack_size(phera_parser::PARSER_STACK_BYTES)
            .spawn_scoped(scope, || pipeline::compile_on_stack(source, file, kind))
            .map_err(|error| CompileError {
                message: error.to_string(),
                file: file.into(),
                line: 1,
                parse: false,
                scanner_diagnostics: Vec::new(),
            })?
            .join()
            .unwrap_or_else(|_| {
                Err(CompileError {
                    message: "Phera internal error: compiler thread failed".into(),
                    file: file.into(),
                    line: 1,
                    parse: false,
                    scanner_diagnostics: Vec::new(),
                })
            })
    })
}
