//! A comparison matrix: each case is called through Phera's argument
//! checks and written into one PHP script, which runs once at the end; the
//! results, output and diagnostics must be the same.

use phera_ext_mbstring::Mbstring;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, NativeError, NativeResult, Registry, Value};

fn registry() -> Registry {
    let mut registry = Registry::new();
    Mbstring.register(&mut registry);
    registry
}

/// How a case's PHP and Phera lines must agree.
#[derive(Clone, Copy)]
enum Comparison {
    Exact,
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
}

pub struct Matrix {
    pub registry: Registry,
    pub context: TestContext,
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
            script: format!("<?php\n{}\n", oracle::PHP_PREAMBLE),
            labels: Vec::new(),
            comparisons: Vec::new(),
            rust: Vec::new(),
        }
    }

    /// Compares `function(...arguments)` with PHP: exactly, except for the
    /// two cases where PHP and Phera cannot agree byte for byte.
    ///
    /// - `mb_list_encodings()`: Phera supports 48 of PHP's 79 encodings, so
    ///   PHP's list is compared restricted to Phera's names. Every listed
    ///   name must be in PHP's list, in PHP's order.
    /// - `iconv()` to a `//TRANSLIT` target; see [`Comparison::Transliteration`].
    pub fn case(&mut self, function: &str, arguments: Vec<Value>) {
        let literals: Vec<String> = arguments.iter().map(oracle::php_literal).collect();
        let mut expression = format!("{function}({})", literals.join(","));
        if function == "mb_list_encodings" && arguments.is_empty() {
            expression = self.php_list_of_phera_names();
        }
        let transliterating = function == "iconv"
            && matches!(arguments.get(1), Some(Value::String(target))
                if target.as_bytes().to_ascii_uppercase().ends_with(b"//TRANSLIT"));
        let comparison = if transliterating {
            Comparison::Transliteration
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
    fn call(&mut self, function: &str, arguments: Vec<Value>) -> NativeResult {
        if let Some(entry) = self.registry.get_function(function) {
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
        }
        call(&self.registry, &mut self.context, function, arguments)
    }

    pub fn compare(self, name: &str) {
        let php = oracle::run_php(name, &self.script).expect("PHP 8.5 is required");
        let shape = |lines: &[String]| -> Vec<String> {
            lines
                .iter()
                .zip(&self.comparisons)
                .map(|(line, comparison)| match comparison {
                    Comparison::Exact => line.clone(),
                    Comparison::Transliteration => transliteration_shape(line),
                })
                .collect()
        };
        oracle::compare(name, &self.labels, &shape(&php), &shape(&self.rust));
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
