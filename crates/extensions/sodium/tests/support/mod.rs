//! Execute the same PHP-level calls and observe any by-reference string mutation.

use std::cell::RefCell;
use std::fmt::Write;
use std::rc::Rc;

use phera_ext_sodium::Sodium;
use phera_extension_api::{
    ClassEntry, NativeFn, NativeResult, PhpArray, Registry, Value, prepare_arguments,
    testing::{TestContext, oracle},
};

pub struct Matrix {
    pub registry: Registry,
    script: String,
    labels: Vec<String>,
    results: Vec<String>,
}

impl Matrix {
    pub fn new() -> Self {
        let mut registry = Registry::new();
        registry.class(ClassEntry::class("Exception"));
        registry.load(&Sodium);
        Self {
            registry,
            script: format!("<?php\n{}\n", oracle::PHP_PREAMBLE),
            labels: Vec::new(),
            results: Vec::new(),
        }
    }

    pub async fn case(&mut self, name: &str, mut arguments: Vec<Value>) {
        let entry = self.registry.get_function(name).unwrap();
        let is_ref = !arguments.is_empty() && entry.params.first().is_some_and(|p| p.by_ref);
        let first = arguments.first().map(oracle::php_literal);
        let mut params = arguments
            .iter()
            .map(oracle::php_literal)
            .collect::<Vec<_>>();
        let reference = is_ref.then(|| {
            params[0] = "$ref".to_owned();
            let reference = Rc::new(RefCell::new(arguments[0].clone()));
            arguments[0] = Value::Reference(reference.clone());
            reference
        });
        let expression = format!("{name}({})", params.join(", "));
        let label = if is_ref {
            format!("{} with $ref = {}", expression, first.as_deref().unwrap())
        } else {
            expression.clone()
        };
        self.labels.push(label);
        if let Some(reference) = &reference {
            let first = first.unwrap();
            writeln!(
                self.script,
                "run('call', function() {{ $ref = {first}; return [{expression}, $ref]; }});"
            )
            .unwrap();
            let mut context = TestContext::default();
            let result = invoke(entry, &mut context, arguments).await.map(|value| {
                let mut result = PhpArray::new();
                result.push(value).unwrap();
                result.push(reference.borrow().clone()).unwrap();
                Value::from(result)
            });
            self.results.push(oracle::encode_case(
                "call",
                &result,
                &context.output,
                &context.diagnostics,
            ));
        } else {
            writeln!(self.script, "run('call', fn() => {expression});").unwrap();
            let mut context = TestContext::default();
            let result = invoke(entry, &mut context, arguments).await;
            self.results.push(oracle::encode_case(
                "call",
                &result,
                &context.output,
                &context.diagnostics,
            ));
        }
    }

    pub async fn vary(&mut self, name: &str, arguments: Vec<Value>) {
        self.case(name, arguments.clone()).await;
        for (index, original) in arguments.iter().enumerate() {
            let variants = match original {
                Value::String(bytes) => {
                    let size = bytes.len();
                    vec![
                        repeated(b'A', size.saturating_sub(1)),
                        repeated(b'A', size + 1),
                        repeated(0xff, size),
                        repeated(0, size),
                        Value::from(""),
                        Value::Null,
                        Value::Int(42),
                        Value::from(PhpArray::new()),
                    ]
                }
                Value::Int(_) => vec![
                    Value::Int(-1),
                    Value::Int(0),
                    Value::Int(1),
                    Value::Int(i64::MAX),
                    Value::from("word"),
                    Value::from(PhpArray::new()),
                ],
                _ => unreachable!(),
            };
            for variant in variants {
                let mut invalid = arguments.clone();
                invalid[index] = variant;
                self.case(name, invalid).await;
            }
        }
    }

    pub fn compare(self, name: &str) {
        let php = oracle::run_php(name, &self.script).unwrap();
        oracle::compare(name, &self.labels, &php, &self.results);
        eprintln!("{name} compared {} calls", self.results.len());
    }
}

async fn invoke(
    entry: &phera_extension_api::FunctionEntry,
    context: &mut TestContext,
    mut arguments: Vec<Value>,
) -> NativeResult {
    prepare_arguments(entry, &mut arguments, context)?;
    match entry.handler {
        NativeFn::Sync(call) => call(context, &arguments),
        NativeFn::Async(call) => call(context, arguments).await,
    }
}

pub fn repeated(byte: u8, size: usize) -> Value {
    Value::from(vec![byte; size])
}
