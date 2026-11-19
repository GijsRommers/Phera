//! A comparison matrix: each case is called through Phera's argument
//! checks and written into one PHP script, which runs once at the end; the
//! results, output and diagnostics must be the same.

use phera_ext_mbstring::Mbstring;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{
    Extension, NativeError, NativeFn, NativeResult, Registry, Value, prepare_arguments,
};

fn registry() -> Registry {
    let mut registry = Registry::new();
    Mbstring.register(&mut registry);
    registry
}

/// How a case's PHP and Phera lines must agree.
#[derive(Clone, Copy)]
enum Comparison {
    Exact,
    ExactSince(u32),
    /// iconv's `//TRANSLIT` is not reproducible across processes, so only
    /// the kind of result and the diagnostics must agree, and a string must
    /// be ASCII:
    ///
    /// - glibc transliterates by the process's `LC_CTYPE`, and PHP sets it
    ///   to `C.UTF-8` at startup (`zend_reset_lc_ctype_locale`) while a Rust
    ///   process keeps `C`: `é` becomes `e` in PHP and `?` in Phera.
    /// - Apple's iconv transliterates by the state of the process, so PHP
    ///   disagrees with itself: `iconv('Windows-1252', 'ASCII//TRANSLIT',
    ///   'العربية')` gives one string under `php -r` and another from a
    ///   script file. Whether it succeeds at all changes with the
    ///   conversions made before, so on macOS an ASCII string and "Detected
    ///   an illegal character" count as the same.
    Transliteration,
    /// iconv's `//IGNORE` on a long string under glibc: PHP's own result
    /// loses valid bytes. glibc stops after each internal buffer in which it
    /// skipped something; PHP then steps one input byte forward and calls
    /// again, dropping the first byte of whatever comes next. Only the kind
    /// of result and the diagnostics must agree; tests/large.rs checks
    /// Phera's bytes against a clean conversion of its own.
    IgnoreOnGlibc,
}

/// From this length, glibc's `//IGNORE` has stopped at internal buffers and
/// PHP's result has gaps; the shorter strings of the matrices fit in one.
const LONG_FOR_GLIBC: usize = 64 * 1024;

pub struct Matrix {
    pub registry: Registry,
    pub context: TestContext,
    /// Runs the async functions, and gives them Tokio's blocking threads.
    runtime: tokio::runtime::Runtime,
    script: String,
    labels: Vec<String>,
    comparisons: Vec<Comparison>,
    rust: Vec<String>,
}

impl Matrix {
    pub fn new() -> Self {
        Self {
            registry: registry(),
            context: TestContext::default(),
            runtime: tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("a Tokio runtime"),
            script: format!("<?php\n{}\n", oracle::PHP_PREAMBLE),
            labels: Vec::new(),
            comparisons: Vec::new(),
            rust: Vec::new(),
        }
    }

    /// Compares `function(...arguments)` with PHP: exactly, except for the
    /// cases where PHP and Phera cannot agree byte for byte.
    ///
    /// - `mb_list_encodings()`: Phera supports 48 of PHP's 79 encodings, so
    ///   PHP's list is compared restricted to Phera's names. Every listed
    ///   name must be in PHP's list, in PHP's order.
    /// - `iconv()` to a `//TRANSLIT` target; see [`Comparison::Transliteration`].
    /// - `iconv()` of a long string to `//IGNORE` on glibc; see
    ///   [`Comparison::IgnoreOnGlibc`].
    pub fn case(&mut self, function: &str, arguments: Vec<Value>) {
        self.case_with_minimum(function, arguments, None);
    }

    /// Runs both calls in order, since later observations can read their state.
    pub fn case_with_minimum(
        &mut self,
        function: &str,
        arguments: Vec<Value>,
        minimum: Option<u32>,
    ) {
        let literals: Vec<String> = arguments.iter().map(oracle::php_literal).collect();
        let mut expression = format!("{function}({})", literals.join(","));
        if function == "mb_list_encodings" && arguments.is_empty() {
            expression = self.php_list_of_phera_names();
        }
        let transliterating = function == "iconv"
            && matches!(arguments.get(1), Some(Value::String(target))
                if target.as_bytes().to_ascii_uppercase().ends_with(b"//TRANSLIT"));
        let ignoring_long_on_glibc = cfg!(target_env = "gnu")
            && function == "iconv"
            && matches!(arguments.get(1), Some(Value::String(target))
                if target.as_bytes().to_ascii_uppercase().ends_with(b"//IGNORE"))
            && matches!(arguments.get(2), Some(Value::String(text)) if text.len() >= LONG_FOR_GLIBC);
        let comparison = if let Some(minimum) = minimum {
            Comparison::ExactSince(minimum)
        } else if transliterating {
            Comparison::Transliteration
        } else if ignoring_long_on_glibc {
            Comparison::IgnoreOnGlibc
        } else {
            Comparison::Exact
        };
        self.add(&expression, function, arguments, comparison);
    }

    /// PHP's `mb_list_encodings()` without the names Phera does not list.
    fn php_list_of_phera_names(&self) -> String {
        let listed = call(
            &self.registry,
            &mut TestContext::default(),
            "mb_list_encodings",
            vec![],
        )
        .expect("mb_list_encodings succeeds");
        format!(
            "array_values(array_filter(mb_list_encodings(), fn($e) => in_array($e, {}, true)))",
            oracle::php_literal(&listed)
        )
    }

    fn add(
        &mut self,
        expression: &str,
        function: &str,
        arguments: Vec<Value>,
        comparison: Comparison,
    ) {
        self.script
            .push_str(&format!("run('case', fn() => {expression});\n"));
        self.context.diagnostics.entries.clear();
        self.context.output.clear();
        let result = self.call(function, arguments);
        self.rust.push(oracle::encode_case(
            "case",
            &result,
            &self.context.output,
            &self.context.diagnostics,
        ));
        self.labels.push(expression.to_string());
        self.comparisons.push(comparison);
    }

    /// Calls the function as PHP code calling it with these literals would.
    ///
    /// A literal cannot be passed to a by-reference parameter: the VM throws
    /// at the call site, before the function's arguments are checked
    /// (`crates/vm/src/calls/sends.rs`). `testing::call` starts after that
    /// step, so it is taken here.
    pub fn call(&mut self, function: &str, mut arguments: Vec<Value>) -> NativeResult {
        let Some(entry) = self.registry.get_function(function) else {
            return call(&self.registry, &mut self.context, function, arguments);
        };
        for (index, argument) in arguments.iter().enumerate() {
            let parameter = entry
                .params
                .get(index)
                .or(entry.params.last().filter(|last| last.variadic));
            if let Some(parameter) = parameter
                && parameter.by_ref
                && !argument.is_reference()
            {
                return Err(NativeError::error(format!(
                    "{function}(): Argument #{} (${}) could not be passed by reference",
                    index + 1,
                    parameter.name
                )));
            }
        }
        match entry.handler {
            NativeFn::Sync(_) => call(&self.registry, &mut self.context, function, arguments),
            // `testing::call` runs only sync functions; this does the same
            // for an async one, awaiting it.
            NativeFn::Async(handler) => {
                prepare_arguments(entry, &mut arguments, &mut self.context)?;
                self.runtime.block_on(handler(&mut self.context, arguments))
            }
        }
    }

    pub fn compare(self, name: &str) {
        let php = oracle::run_php(name, &self.script).expect("PHP 8.5 is required");
        assert_eq!(
            php.len(),
            self.rust.len(),
            "{name}: different number of lines"
        );
        let mut labels = Vec::new();
        let mut expected = Vec::new();
        let mut actual = Vec::new();
        for (index, (((php, rust), label), comparison)) in php
            .into_iter()
            .zip(self.rust)
            .zip(self.labels)
            .zip(self.comparisons)
            .enumerate()
        {
            let (php, rust) = match comparison {
                Comparison::Exact => (php, rust),
                Comparison::ExactSince(minimum) => {
                    if !oracle::supports_php_since(&format!("{name} #{index} {label}"), minimum) {
                        continue;
                    }
                    (php, rust)
                }
                Comparison::Transliteration => {
                    (transliteration_shape(&php), transliteration_shape(&rust))
                }
                Comparison::IgnoreOnGlibc => {
                    (kind_and_diagnostics(&php), kind_and_diagnostics(&rust))
                }
            };
            labels.push(label);
            expected.push(php);
            actual.push(rust);
        }
        oracle::compare(name, &labels, &expected, &actual);
    }
}

/// A case line with a transliterated result reduced to what is
/// reproducible; see [`Comparison::Transliteration`].
fn transliteration_shape(line: &str) -> String {
    const REFUSED: &str = "case=B:0|N:iconv(): Detected an illegal character in input string";
    if cfg!(target_vendor = "apple") && line == REFUSED {
        return "case=(ASCII string or refused)".to_string();
    }
    let Some(rest) = line.strip_prefix("case=S:") else {
        return line.to_string();
    };
    let (hex, diagnostics) = rest.split_at(rest.find('|').unwrap_or(rest.len()));
    // Hexadecimal pairs from 00 to 7F.
    let ascii = hex.as_bytes().chunks(2).all(|digits| digits[0] < b'8');
    if !ascii {
        return line.to_string();
    }
    if cfg!(target_vendor = "apple") && diagnostics.is_empty() {
        return "case=(ASCII string or refused)".to_string();
    }
    format!("case=(ASCII string){diagnostics}")
}

/// A case line with a string result reduced to its being a string.
fn kind_and_diagnostics(line: &str) -> String {
    let Some(rest) = line.strip_prefix("case=S:") else {
        return line.to_string();
    };
    let diagnostics = &rest[rest.find('|').unwrap_or(rest.len())..];
    format!("case=(string){diagnostics}")
}
