//! Run a batch through PHP and the same registered bindings in Rust.

use std::cell::RefCell;
use std::fmt::Write;
use std::rc::Rc;

use phera_ext_intl::Intl;
use phera_extension_api::testing::{TestContext, call, call_method, instantiate, oracle};
use phera_extension_api::{ArrayKey, ClassEntry, PhpArray, Registry, Value};

pub struct Matrix {
    registry: Registry,
    script: String,
    labels: Vec<String>,
    results: Vec<String>,
}

impl Matrix {
    pub fn new() -> Self {
        let mut registry = Registry::new();
        registry.class(ClassEntry::class("Exception"));

        registry.load(&Intl);
        let script = format!("<?php\n{}\n", oracle::PHP_PREAMBLE);
        Self {
            registry,
            script,
            labels: Vec::new(),
            results: Vec::new(),
        }
    }

    pub fn case(&mut self, function: &str, arguments: &[Value]) {
        self.record(function, arguments, None);
    }

    pub fn record(&mut self, function: &str, arguments: &[Value], next: Option<Value>) {
        self.run(function, arguments, next, None);
    }

    pub fn instance(
        &mut self,
        class: &str,
        constructor: &[Value],
        method: &str,
        arguments: &[Value],
        next: Option<Value>,
    ) {
        self.run(method, arguments, next, Some((class, constructor)));
    }

    fn run(
        &mut self,
        function: &str,
        arguments: &[Value],
        next: Option<Value>,
        instance: Option<(&str, &[Value])>,
    ) {
        let tag = self.labels.len().to_string();
        let mut php_arguments: Vec<_> = arguments.iter().map(oracle::php_literal).collect();
        let setup = next.as_ref().map_or(String::new(), |next| {
            php_arguments.push("$next".to_owned());
            format!("$next = {};", oracle::php_literal(next))
        });
        let target = if instance.is_some() {
            format!("$object->{function}")
        } else {
            function.to_owned()
        };
        let construct = instance.map_or_else(String::new, |(class, constructor)| {
            let values: Vec<_> = constructor.iter().map(oracle::php_literal).collect();
            format!("$object = new {class}({});", values.join(", "))
        });
        let call_text = format!("{target}({})", php_arguments.join(", "));
        let changed = if next.is_some() { ", $next" } else { "" };
        let reads_error = !function.starts_with("intl_");
        let get_error = if reads_error {
            ", intl_get_error_code(), intl_get_error_message()"
        } else {
            ""
        };
        writeln!(
            self.script,
            "normalizer_normalize(''); run('{tag}', function() {{ {setup} {construct} \
             return [{call_text}{get_error}{changed}]; }});"
        )
        .unwrap();
        self.labels.push(format!("{construct} {call_text}"));

        let mut context = TestContext::default();
        let mut arguments = arguments.to_vec();
        let next = next.map(|initial| Rc::new(RefCell::new(initial)));
        if let Some(next) = &next {
            arguments.push(Value::Reference(Rc::clone(next)));
        }
        let result = if let Some((class, constructor)) = instance {
            let object = instantiate(&self.registry, class).unwrap();
            call_method(
                &self.registry,
                &mut context,
                &object,
                "__construct",
                constructor.to_vec(),
            )
            .and_then(|_| call_method(&self.registry, &mut context, &object, function, arguments))
        } else if let Some((class, method)) = function.split_once("::") {
            let object = instantiate(&self.registry, class).unwrap();
            call_method(&self.registry, &mut context, &object, method, arguments)
        } else {
            call(&self.registry, &mut context, function, arguments)
        };
        let result = result.map(|value| {
            let mut values = vec![value];
            if reads_error {
                for getter in ["intl_get_error_code", "intl_get_error_message"] {
                    values.push(call(&self.registry, &mut context, getter, vec![]).unwrap());
                }
            }
            if let Some(next) = next {
                values.push(next.borrow().clone());
            }
            let array: PhpArray = values
                .into_iter()
                .enumerate()
                .map(|(index, value)| (ArrayKey::Int(index as i64), value))
                .collect();
            Value::from(array)
        });
        self.results.push(oracle::encode_case(
            &tag,
            &result,
            &context.output,
            &context.diagnostics,
        ));
    }

    pub fn compare(&self, name: &str) {
        let php = oracle::run_php(name, &self.script).expect("PHP 8.5 with intl required");
        oracle::compare(name, &self.labels, &php, &self.results);
    }
}
