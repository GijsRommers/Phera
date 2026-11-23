//! Records native calls and serializes the same calls for the PHP oracle.

use std::cell::RefCell;
use std::rc::Rc;

use phera_ext_openssl::OpenSsl;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Registry, Value};

pub struct Matrix {
    script: String,
    lines: Vec<String>,
    labels: Vec<String>,
    registry: Registry,
    context: TestContext,
}

impl Matrix {
    pub fn new() -> Self {
        let mut registry = Registry::new();
        registry.load(&OpenSsl);
        Self {
            script: format!("<?php\n{}", oracle::PHP_PREAMBLE),
            lines: Vec::new(),
            labels: Vec::new(),
            registry,
            context: TestContext::default(),
        }
    }

    pub fn case(
        &mut self,
        function: &str,
        mut args: Vec<Value>,
        output: Option<usize>,
    ) -> Option<(Value, Value)> {
        use std::fmt::Write;
        let context = &mut self.context;
        context.output.clear();
        context.diagnostics.entries.clear();
        let label = self.lines.len().to_string();
        let mut literals: Vec<_> = args.iter().map(oracle::php_literal).collect();
        let reference = output.map(|index| {
            let value = Rc::new(RefCell::new(args[index].clone()));
            args[index] = Value::Reference(value.clone());
            write!(
                self.script,
                "run('{label}', function() {{ $ref = {}; ",
                literals[index]
            )
            .unwrap();
            literals[index] = "$ref".into();
            value
        });
        let expression = format!("{function}({})", literals.join(", "));
        if reference.is_some() {
            writeln!(
                self.script,
                "try {{ return {expression}; }} finally {{ echo enc($ref); }} }});"
            )
            .unwrap();
        } else {
            writeln!(self.script, "run('{label}', fn() => {expression});").unwrap();
        }
        let result = call(&self.registry, context, function, args);
        let value = reference.map_or(Value::Null, |reference| {
            let value = reference.borrow().clone();
            context
                .output
                .extend_from_slice(oracle::encode(&value).as_bytes());
            value
        });
        self.lines.push(oracle::encode_case(
            &label,
            &result,
            &context.output,
            &context.diagnostics,
        ));
        self.labels.push(expression.chars().take(280).collect());
        result.ok().map(|result| (result, value))
    }

    pub fn check(self, name: &str) {
        let expected = oracle::run_php(name, &self.script).expect("PHP oracle required");
        println!("{name}: {} compared calls", self.lines.len());
        oracle::compare(name, &self.labels, &expected, &self.lines);
    }
}
