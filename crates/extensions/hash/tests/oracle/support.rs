//! What the hash tests share: the registry, a call that also awaits async
//! functions, and [`Comparison`], which builds one PHP script and the
//! matching Rust lines case by case.

use phera_ext_hash::Hash;
use phera_extension_api::testing::{TestContext, oracle};
use phera_extension_api::{Extension, NativeFn, NativeResult, Registry, Value, prepare_arguments};

pub fn registry() -> Registry {
    let mut registry = Registry::new();
    Hash.register(&mut registry);
    registry
}

/// Calls a native function the way the VM does, sync or async.
/// `testing::call` covers only sync functions.
pub async fn invoke(
    registry: &Registry,
    context: &mut TestContext,
    name: &str,
    mut arguments: Vec<Value>,
) -> NativeResult {
    let entry = registry.get_function(name).expect("registered function");
    prepare_arguments(entry, &mut arguments, context)?;
    match entry.handler {
        NativeFn::Sync(function) | NativeFn::DeprecatedSync(function) => {
            function(context, &arguments)
        }
        NativeFn::Async(function) => function(context, arguments).await,
    }
}

/// `name(arguments)` as PHP source.
pub fn php_call(name: &str, arguments: &[Value]) -> String {
    let arguments: Vec<String> = arguments.iter().map(oracle::php_literal).collect();
    format!("{name}({})", arguments.join(","))
}

/// One PHP script with a `run()` line per case, and the line Phera produced
/// for each case, compared all at once by [`Comparison::compare`].
pub struct Comparison {
    script: String,
    rust: Vec<String>,
    labels: Vec<String>,
}

impl Comparison {
    pub fn new() -> Self {
        Comparison {
            script: format!("<?php\n{}\n", oracle::PHP_PREAMBLE),
            rust: Vec::new(),
            labels: Vec::new(),
        }
    }

    /// Records one case: PHP runs `closure`, Phera gave `result`. The
    /// context's output and diagnostics are cleared afterwards, as PHP's
    /// `run()` clears them between cases.
    pub fn record(&mut self, closure: &str, result: &NativeResult, context: &mut TestContext) {
        self.script.push_str(&format!("run('case', {closure});\n"));
        self.rust.push(oracle::encode_case(
            "case",
            result,
            &context.output,
            &context.diagnostics,
        ));
        self.labels.push(closure.to_string());
        context.output.clear();
        context.diagnostics.entries.clear();
    }

    /// Records a case whose PHP side is the single expression `expression`.
    pub fn record_expression(
        &mut self,
        expression: &str,
        result: &NativeResult,
        context: &mut TestContext,
    ) {
        self.record(&format!("fn() => {expression}"), result, context);
    }

    /// Calls `name` in Phera, sync or async, and records the same call for
    /// PHP.
    pub async fn record_call(
        &mut self,
        registry: &Registry,
        context: &mut TestContext,
        name: &str,
        arguments: Vec<Value>,
    ) {
        let expression = php_call(name, &arguments);
        let result = invoke(registry, context, name, arguments).await;
        self.record_expression(&expression, &result, context);
    }

    /// Runs the script in PHP and fails on the first differing lines.
    /// `name` keeps the script files of different tests apart.
    pub fn compare(self, name: &str, what: &str) {
        let php = oracle::run_php(name, &self.script).expect("PHP 8.5 required");
        oracle::compare(what, &self.labels, &php, &self.rust);
    }
}
