//! Every ctype function against PHP 8.5: each byte alone and followed by a
//! set of others, the integers from -300 to 512, and the other types.

use phera_ext_ctype::Ctype;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{PhpArray, Registry, Value};
use std::rc::Rc;

const FUNCTIONS: [&str; 11] = [
    "ctype_alnum",
    "ctype_alpha",
    "ctype_cntrl",
    "ctype_digit",
    "ctype_graph",
    "ctype_lower",
    "ctype_print",
    "ctype_punct",
    "ctype_space",
    "ctype_upper",
    "ctype_xdigit",
];

/// The argument of each case, the same for every function.
fn arguments() -> Vec<Value> {
    let mut arguments = Vec::new();
    for byte in 0..=255u8 {
        arguments.push(Value::from(vec![byte]));
        for next in [0, 9, 10, 11, 12, 32, 48, 65, 97, 127, 128, 255] {
            arguments.push(Value::from(vec![byte, next]));
        }
    }
    arguments.extend((-300..=512).map(Value::Int));
    arguments.extend([
        Value::Null,
        Value::Bool(false),
        Value::Bool(true),
        Value::Float(1.0),
        Value::Float(f64::NAN),
        Value::Float(f64::INFINITY),
        Value::Int(i64::MIN),
        Value::Int(i64::MAX),
        Value::from(""),
        Value::Array(Rc::new(PhpArray::new())),
    ]);
    arguments
}

#[test]
fn php_oracle() {
    let mut registry = Registry::new();
    registry.load(&Ctype);
    // The classes are those of the C locale.
    let mut script = format!(
        "<?php\nsetlocale(LC_CTYPE, 'C');\n{}\n",
        oracle::PHP_PREAMBLE
    );
    let mut labels = Vec::new();
    let mut rust = Vec::new();
    for name in FUNCTIONS {
        for argument in arguments() {
            let expression = format!("{name}({})", oracle::php_literal(&argument));
            script.push_str(&format!("run('case', fn() => {expression});\n"));
            let mut context = TestContext::default();
            let result = call(&registry, &mut context, name, vec![argument]);
            rust.push(oracle::encode_case(
                "case",
                &result,
                &context.output,
                &context.diagnostics,
            ));
            labels.push(expression);
        }
    }
    let php = oracle::run_php("ctype", &script).expect("PHP 8.5 oracle required");
    oracle::compare("ctype", &labels, &php, &rust);
}
