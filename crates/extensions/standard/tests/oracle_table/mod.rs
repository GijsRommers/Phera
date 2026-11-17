//! A table of cases compared with real PHP, for the oracle tests of this
//! crate that call functions directly (the callback tests have their own
//! harness, since `TestContext` cannot call PHP code).
//!
//! Each case becomes one `run()` line of a PHP script built on
//! `oracle::PHP_PREAMBLE`, and the same call through `testing::call`;
//! [`OracleTable::finish`] runs PHP once and compares every line.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use phera_ext_standard::Standard;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, NativeResult, Registry, Value};

pub struct OracleTable {
    registry: Registry,
    script: String,
    labels: Vec<String>,
    rust: Vec<String>,
}

impl OracleTable {
    pub fn new() -> Self {
        let mut registry = Registry::new();
        Standard.register(&mut registry);
        Self {
            registry,
            script: format!("<?php\n{}", oracle::PHP_PREAMBLE),
            labels: Vec::new(),
            rust: Vec::new(),
        }
    }

    /// `name(arguments…)`, with the arguments as PHP literals.
    pub fn check(&mut self, name: &str, arguments: Vec<Value>) {
        let expression = format!("{name}({})", literals(&arguments));
        let closure = format!("fn() => {expression}");
        self.check_custom(name, &closure, expression, arguments);
    }

    /// A call whose PHP side is written by hand, as a closure for `run()`,
    /// for arguments that have no literal.
    pub fn check_custom(
        &mut self,
        name: &str,
        php_closure: &str,
        label: String,
        arguments: Vec<Value>,
    ) {
        let mut context = TestContext::default();
        let result = call(&self.registry, &mut context, name, arguments);
        self.add(name, php_closure, label, &result, &context);
    }

    /// `name(arguments…)` with the argument at `index` passed by reference:
    /// a variable that starts as that argument. Compares the result and the
    /// variable afterwards, as `[result, variable]`.
    pub fn check_by_reference(&mut self, name: &str, mut arguments: Vec<Value>, index: usize) {
        let initial = oracle::php_literal(&arguments[index]);
        let mut php_arguments: Vec<String> = arguments.iter().map(oracle::php_literal).collect();
        php_arguments[index] = "$v".to_string();
        let expression = format!("{name}({})", php_arguments.join(", "));
        let closure =
            format!("function() {{ $v = {initial}; $r = {expression}; return [$r, $v]; }}");
        let variable = Rc::new(RefCell::new(arguments[index].clone()));
        arguments[index] = Value::Reference(variable.clone());
        let mut context = TestContext::default();
        let result = call(&self.registry, &mut context, name, arguments)
            .map(|result| pair(result, variable.borrow().clone()));
        let label = format!("{expression} with $v = {initial}");
        self.add(name, &closure, label, &result, &context);
    }

    fn add(
        &mut self,
        tag: &str,
        php_closure: &str,
        label: String,
        result: &NativeResult,
        context: &TestContext,
    ) {
        self.script
            .push_str(&format!("run('{tag}', {php_closure});\n"));
        self.labels.push(label);
        self.rust.push(oracle::encode_case(
            tag,
            result,
            &context.output,
            &context.diagnostics,
        ));
    }

    /// Runs PHP, fails on the first 30 differences, and prints how many
    /// cases each function had.
    pub fn finish(self, what: &str) {
        let file = format!("standard-{}", what.replace(' ', "-"));
        let php = oracle::run_php(&file, &self.script).expect("PHP oracle required");
        oracle::compare(what, &self.labels, &php, &self.rust);
        let mut counts = BTreeMap::new();
        for line in &self.rust {
            if let Some((tag, _)) = line.split_once('=') {
                *counts.entry(tag).or_insert(0_usize) += 1;
            }
        }
        for (tag, count) in counts {
            println!("{tag}: {count} cases");
        }
    }
}

/// The arguments as PHP literals, separated by commas.
pub fn literals(arguments: &[Value]) -> String {
    let literals: Vec<String> = arguments.iter().map(oracle::php_literal).collect();
    literals.join(", ")
}

/// `[first, second]`, as the PHP side returns two values.
pub fn pair(first: Value, second: Value) -> Value {
    let mut array = phera_extension_api::PhpArray::new();
    array.push(first).expect("room for two values");
    array.push(second).expect("room for two values");
    Value::from(array)
}
