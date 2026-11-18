//! What the random tests share: the registry, engines and randomizers made
//! the way PHP code makes them, and [`Comparison`], which builds one PHP
//! script and the matching Rust lines case by case.

use std::rc::Rc;

use phera_extension_api::testing::{TestContext, call_method, instantiate, oracle};
use phera_extension_api::{Extension, NativeResult, Object, Registry, Value};

/// Core is needed too: the random classes extend `Error` and `Exception`.
pub fn registry() -> Registry {
    let mut registry = Registry::new();
    phera_ext_core::Core.register(&mut registry);
    phera_ext_random::Random.register(&mut registry);
    registry
}

/// `new Random\Engine\$class(...$arguments)`
pub fn engine(
    registry: &Registry,
    context: &mut TestContext,
    class: &str,
    arguments: Vec<Value>,
) -> Rc<Object> {
    let engine = instantiate(registry, &format!("Random\\Engine\\{class}")).expect("engine class");
    call_method(registry, context, &engine, "__construct", arguments).expect("engine constructed");
    engine
}

/// `new Random\Randomizer($engine)`
pub fn randomizer(
    registry: &Registry,
    context: &mut TestContext,
    engine: &Rc<Object>,
) -> Rc<Object> {
    let randomizer = instantiate(registry, "Random\\Randomizer").expect("Randomizer class");
    let arguments = vec![Value::Object(engine.clone())];
    call_method(registry, context, &randomizer, "__construct", arguments)
        .expect("Randomizer constructed");
    randomizer
}

/// The arguments as PHP source, separated by commas.
pub fn php_arguments(arguments: &[Value]) -> String {
    let arguments: Vec<String> = arguments.iter().map(oracle::php_literal).collect();
    arguments.join(",")
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

    /// PHP code that runs before the next case, such as `mt_srand(42);`.
    pub fn setup(&mut self, php: &str) {
        self.script.push_str(php);
        self.script.push('\n');
    }

    /// Records one case: PHP evaluates `expression`, Phera gave `result`.
    /// The context's output and diagnostics are cleared afterwards, as
    /// PHP's `run()` clears them between cases.
    pub fn record(&mut self, expression: &str, result: &NativeResult, context: &mut TestContext) {
        self.record_closure(&format!("fn() => {expression}"), result, context);
    }

    /// Like [`Comparison::record`], with a whole PHP closure.
    pub fn record_closure(
        &mut self,
        closure: &str,
        result: &NativeResult,
        context: &mut TestContext,
    ) {
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

    /// Runs the script in PHP and fails on the first differing lines.
    /// `name` keeps the script files of different tests apart.
    pub fn compare(self, name: &str, what: &str) {
        let php = oracle::run_php(name, &self.script).expect("PHP 8.5 required");
        oracle::compare(what, &self.labels, &php, &self.rust);
    }
}
