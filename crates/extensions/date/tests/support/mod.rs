//! Every call compares values, diagnostics and exception text against real PHP 8.5.
use phera_ext_date::Date;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Registry, Value};

pub struct Matrix {
    pub context: TestContext,
    registry: Registry,
    script: String,
    labels: Vec<String>,
    rust: Vec<String>,
}

impl Matrix {
    pub fn new() -> Self {
        let mut registry = Registry::new();
        registry.load(&phera_ext_core::Core);
        registry.load(&Date);
        Self {
            context: TestContext::default(),
            registry,
            script: format!("<?php\n{}", oracle::PHP_PREAMBLE),
            labels: Vec::new(),
            rust: Vec::new(),
        }
    }

    pub fn case(&mut self, function: &str, arguments: Vec<Value>) {
        self.context.diagnostics.entries.clear();
        self.context.output.clear();
        let expression = format!(
            "{function}({})",
            arguments
                .iter()
                .map(oracle::php_literal)
                .collect::<Vec<_>>()
                .join(",")
        );
        self.script
            .push_str(&format!("run('case', fn()=>{expression});\n"));
        let result = call(&self.registry, &mut self.context, function, arguments);
        self.rust.push(oracle::encode_case(
            "case",
            &result,
            &self.context.output,
            &self.context.diagnostics,
        ));
        self.labels.push(expression);
    }

    pub fn finish(self, name: &str) {
        let php = oracle::run_php(name, &self.script).expect("PHP 8.5 oracle required");
        let mismatches = self
            .labels
            .iter()
            .zip(php.iter().zip(&self.rust))
            .filter(|(_, (php, rust))| php != rust)
            .map(|(label, (php, rust))| format!("{label}\tPHP={php}\tRust={rust}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(
            std::env::temp_dir().join(format!("{name}-mismatches.txt")),
            mismatches,
        )
        .unwrap();
        oracle::compare(name, &self.labels, &php, &self.rust);
    }
}
