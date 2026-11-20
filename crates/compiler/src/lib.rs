//! Compile PHP source into immutable register bytecode on a dedicated stack.

mod compile_error;
mod constant_callables;
mod context;
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

/// Compiles on a caller-provided stack of at least the parser's stack size.
/// A preload worker can reuse that stack for all the files it compiles.
pub fn compile_bytes_with_natives_on_stack(
    source: &[u8],
    file: &str,
    native_functions: &NativeFunctions<'_>,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    pipeline::compile_on_stack(
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

/// Compile a script under PHP's CLI scanner rules, including its first-line shebang.
pub fn compile_cli_bytes(
    source: &[u8],
    file: &str,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    compile_cli_source(source, file, None)
}

/// Compile a CLI script with the native registry used for that request.
pub fn compile_cli_bytes_with_natives(
    source: &[u8],
    file: &str,
    native_functions: &NativeFunctions<'_>,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    compile_cli_source(source, file, Some(native_functions))
}

fn compile_cli_source(
    source: &[u8],
    file: &str,
    native_functions: Option<&NativeFunctions<'_>>,
) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    // The CLI scanner counts through the first LF as one line, even if a
    // carriage return occurs inside the shebang. Keep the byte offsets.
    let line_end = source
        .starts_with(b"#!")
        .then(|| source.iter().position(|&byte| byte == b'\n'))
        .flatten();
    if let Some(end) = line_end {
        let line = &source[..end];
        let prefix = line.strip_suffix(b"\r").unwrap_or(line);
        if prefix.contains(&b'\r') {
            let mut normalized = source.to_vec();
            for byte in &mut normalized[..prefix.len()] {
                if *byte == b'\r' {
                    *byte = b' ';
                }
            }
            return compile_source(
                &normalized,
                file,
                pipeline::SourceKind::CliFile,
                native_functions,
            );
        }
    }
    compile_source(
        source,
        file,
        pipeline::SourceKind::CliFile,
        native_functions,
    )
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
