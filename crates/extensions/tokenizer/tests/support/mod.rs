//! Native calls and their PHP oracle equivalents, e.g. `PhpToken::tokenize($source)`.

use phera_ext_core::Core;
use phera_ext_tokenizer::Tokenizer;
use phera_extension_api::{
    Extension, NativeError, NativeResult, Registry, Value,
    testing::{TestContext, call, call_method, instantiate, oracle},
};

pub struct Matrix {
    pub php: String,
    pub results: Vec<String>,
    pub labels: Vec<String>,
    pub token_count: usize,
    pub registry: Registry,
    oracle_file: String,
    next_line: usize,
}

impl Matrix {
    pub fn new(oracle_name: &str) -> Self {
        let mut registry = Registry::new();
        Core.register(&mut registry);
        Tokenizer.register(&mut registry);
        let php = format!("<?php\n{}", oracle::PHP_PREAMBLE);
        let next_line = 1 + php.bytes().filter(|&byte| byte == b'\n').count();
        Self {
            php,
            next_line,
            oracle_file: oracle::script_path(oracle_name)
                .to_string_lossy()
                .into_owned(),
            results: Vec::new(),
            labels: Vec::new(),
            token_count: 0,
            registry,
        }
    }

    pub fn check(&mut self, label: String, name: &str, arguments: Vec<Value>) {
        let php_args = arguments
            .iter()
            .map(oracle::php_literal)
            .collect::<Vec<_>>()
            .join(", ");
        let expression = format!("{name}({php_args})");
        let mut context = TestContext {
            caller: Some((self.oracle_file.clone(), self.next_line)),
            ..TestContext::default()
        };
        let result = if let Some(method) = name.strip_prefix("PhpToken::") {
            let object = instantiate(&self.registry, "PhpToken").unwrap();
            call_method(&self.registry, &mut context, &object, method, arguments)
        } else {
            call(&self.registry, &mut context, name, arguments)
        };

        if let Ok(Value::Array(tokens)) = &result {
            self.token_count += tokens.len();
        }
        self.record(label, &expression, result, &context);
    }

    pub fn record(
        &mut self,
        label: String,
        expression: &str,
        result: NativeResult,
        context: &TestContext,
    ) {
        let index = self.labels.len();
        self.labels.push(label);
        self.php
            .push_str(&format!("run('{index}', fn() => {expression});\n"));
        self.next_line += 1 + expression.bytes().filter(|&byte| byte == b'\n').count();
        let result = result.map_err(|error| match error.into_exception_parts() {
            Ok((class, message, _, _)) => NativeError::throw(class, message),
            Err(error) => error,
        });
        self.results.push(oracle::encode_case(
            &index.to_string(),
            &result,
            &context.output,
            &context.diagnostics,
        ));
    }
}
