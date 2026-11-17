//! Oracle assembly shared by the callback matrices in this test binary.

use std::cell::RefCell;
use std::rc::Rc;

use crate::callback_support::{Harness, PHP_CALLBACKS, list, ready};
use phera_extension_api::testing::oracle;
use phera_extension_api::{NativeResult, Value};

pub struct Matrix {
    script: String,
    labels: Vec<String>,
    rust: Vec<String>,
}

impl Matrix {
    pub fn new() -> Self {
        Self {
            script: format!("<?php\n{}\n{PHP_CALLBACKS}\n", oracle::PHP_PREAMBLE),
            labels: Vec::new(),
            rust: Vec::new(),
        }
    }

    pub fn check(
        &mut self,
        name: &str,
        arguments: Vec<Value>,
        fixed: Option<Value>,
        throw_at: Option<usize>,
        mutate: bool,
    ) {
        let reference = name.ends_with("sort") || name.starts_with("array_walk");
        let array = arguments.first().cloned().unwrap_or(Value::Null);
        let target = Value::Reference(Rc::new(RefCell::new(array.clone())));
        let mut parameters = arguments.clone();
        if reference && !parameters.is_empty() {
            parameters[0] = target.clone();
        }
        let mut context = Harness {
            fixed: fixed.clone(),
            throw_at,
            target: Some(target.clone()),
            mutate,
            ..Harness::default()
        };
        let result = ready(context.invoke(name, parameters));
        let mut php_args = arguments
            .iter()
            .map(oracle::php_literal)
            .collect::<Vec<_>>();
        if reference && !php_args.is_empty() {
            php_args[0] = "$TARGET".into();
        }
        let expression = format!("{name}({})", php_args.join(","));
        let tracked = arguments.iter().any(|value| {
            matches!(value, Value::String(string)
            if string.as_bytes().starts_with(b"probe_"))
        });
        // Native callbacks can be compared directly; PHP closures record the
        // same argument trace as Context::call for the richer mutation cases.
        let trace = if tracked {
            list(context.trace.clone())
        } else {
            list([])
        };
        let error = match &result {
            Ok(value) => list([Value::Bool(true), value.clone()]),
            Err(phera_extension_api::NativeError::Throw { class, message, .. }) => list([
                Value::Bool(false),
                Value::from(*class),
                Value::from(message.as_str()),
            ]),
            Err(error) => panic!("unexpected unwind: {error:?}"),
        };
        let outcome: NativeResult = Ok(list([
            error,
            if reference {
                target.deref().clone()
            } else {
                Value::Null
            },
            trace,
        ]));
        self.rust.push(oracle::encode_case(
            name,
            &outcome,
            &context.base.output,
            &context.base.diagnostics,
        ));
        self.labels.push(expression.clone());
        let fixed_literal = fixed.as_ref().map_or("null".into(), oracle::php_literal);
        self.script.push_str(&format!(
            "$TRACE=[];$CALLS=0;$HAS_FIXED={};$FIXED={fixed_literal};$THROW_AT={};\n\
             $MUTATE={mutate};$TARGET={};\n\
             run('{name}', function() {{global $TARGET,$TRACE;\n\
             try {{$out=[true,{expression}];}}\
             catch(Throwable $e) {{$out=[false,get_class($e),$e->getMessage()];}}\n\
             return [$out,{},{}]; }});\n",
            fixed.is_some(),
            throw_at.map_or("null".into(), |value| value.to_string()),
            oracle::php_literal(&array),
            if reference { "$TARGET" } else { "null" },
            if tracked { "$TRACE" } else { "[]" },
        ));
    }

    pub fn finish(self, name: &str) {
        let php = oracle::run_php(name, &self.script).expect("PHP 8.5 oracle required");
        oracle::compare(name, &self.labels, &php, &self.rust);
    }
}
