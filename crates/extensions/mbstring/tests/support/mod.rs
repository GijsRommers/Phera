//! Every case runs through argument preparation and captures all diagnostics.
use phera_ext_mbstring::Mbstring;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, Registry, Value};
pub struct Matrix {
    pub registry: Registry,
    pub context: TestContext,
    pub script: String,
    pub labels: Vec<String>,
    pub rust: Vec<String>,
}
impl Matrix {
    pub fn new() -> Self {
        let mut registry = Registry::new();
        Mbstring.register(&mut registry);
        Self {
            registry,
            context: TestContext::default(),
            script: format!("<?php\n{}\n", oracle::PHP_PREAMBLE),
            labels: Vec::new(),
            rust: Vec::new(),
        }
    }
    pub fn case(&mut self, function: &str, arguments: Vec<Value>) {
        let expression = format!(
            "{function}({})",
            arguments
                .iter()
                .map(oracle::php_literal)
                .collect::<Vec<_>>()
                .join(",")
        );
        self.case_as(&expression, function, arguments);
    }

    /// A case whose PHP side runs `expression` instead of the plain call,
    /// for a result that can only be compared after PHP has adapted it.
    pub fn case_as(&mut self, expression: &str, function: &str, arguments: Vec<Value>) {
        self.script
            .push_str(&format!("run('case', fn() => {expression});\n"));
        self.context.diagnostics.entries.clear();
        self.context.output.clear();
        let result = call(&self.registry, &mut self.context, function, arguments);
        self.rust.push(oracle::encode_case(
            "case",
            &result,
            &self.context.output,
            &self.context.diagnostics,
        ));
        self.labels.push(expression.to_string());
    }
    pub fn compare(self, name: &str) {
        let php = oracle::run_php(name, &self.script).expect("PHP 8.5 required");
        let failures: Vec<_> = php
            .iter()
            .zip(&self.rust)
            .zip(&self.labels)
            .filter(|((p, r), _)| p != r)
            .map(|((p, r), label)| format!("{label}\nPHP {p}\nRust {r}\n"))
            .collect();
        std::fs::write(
            std::env::temp_dir().join(format!("{name}-mismatches.txt")),
            failures.join("\n"),
        )
        .unwrap();
        oracle::compare(name, &self.labels, &php, &self.rust);
    }
}
