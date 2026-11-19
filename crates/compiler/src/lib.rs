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
mod reflection_initializer;
mod statements;
mod temporaries;

#[cfg(test)]
mod tests;

pub use compile_error::CompileError;
pub use reflection_initializer::{CompiledInitializer, compile_reflection_initializer};

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
    compile_source(source, file, pipeline::SourceKind::File, None)
}

/// Optimize calls to standard native functions enabled for this runtime.
pub fn compile_bytes_with_natives(
    source: &[u8],
    file: &str,
    native_functions: &NativeFunctions<'_>,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    compile_source(
        source,
        file,
        pipeline::SourceKind::File,
        Some(native_functions),
    )
}

/// Registry facts shared with the compiler thread for calls such as
/// `strlen($text)` and `extension_loaded('Core')`.
pub struct NativeFunctions<'a> {
    pub lookup: &'a (dyn Fn(&str) -> Option<NativeFunction> + Sync),
    pub fold_string_call: &'a StringCallEvaluator<'a>,
}

type StringCallEvaluator<'a> = dyn Fn(&str, &[u8]) -> Option<phera_bytecode::Constant> + Sync + 'a;

/// Which call shape can bypass argument routing for an enabled function.
#[derive(Clone, Copy)]
pub enum NativeFunction {
    General,
    Direct { positional_by_value: usize },
}

/// Compile the code passed to `eval()` without requiring an opening tag.
pub fn compile_eval(
    source: &[u8],
    file: &str,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    compile_eval_with_seed(source, file, 0)
}

/// Repeated evals at the same source location declare distinct anonymous classes.
pub fn compile_eval_with_seed(
    source: &[u8],
    file: &str,
    anonymous_seed: u64,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    let code = [b"<?php ".as_slice(), source].concat();
    compile_source(
        &code,
        file,
        pipeline::SourceKind::Eval { anonymous_seed },
        None,
    )
}

fn compile_source(
    source: &[u8],
    file: &str,
    kind: pipeline::SourceKind,
    native_functions: Option<&NativeFunctions<'_>>,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    on_compile_stack(file, || {
        pipeline::compile_on_stack(source, file, kind, native_functions)
    })
}

fn on_compile_stack<T: Send>(
    file: &str,
    compile: impl FnOnce() -> Result<T, CompileError> + Send,
) -> Result<T, CompileError> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("phera-compiler".into())
            .stack_size(phera_parser::PARSER_STACK_BYTES)
            .spawn_scoped(scope, compile)
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
