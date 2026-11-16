//! Object-mode oracle tests inject a test-owned stdClass at the private boundary.
use super::parser::parse;
use crate::flags::Flags;
use phera_extension_api::testing::oracle;
use phera_extension_api::value::ClassHandle;
use phera_extension_api::{Object, PhpArray, Value};
use std::{any::Any, rc::Rc};
/// The test supplies class ownership; production must obtain it from the VM.
struct StandardClass;
impl ClassHandle for StandardClass {
    fn name(&self) -> &str {
        "stdClass"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
fn factory(properties: PhpArray) -> Value {
    let object = Object::new(1, Rc::new(StandardClass));
    for (key, value) in properties.iter() {
        object.set(key.clone(), value.clone());
    }
    Value::Object(Rc::new(object))
}
#[test]
fn object_parser_matches_php() {
    let mut script = format!(
        "<?php\n{}",
        oracle::PHP_PREAMBLE.replace("foreach ((array) $v as", "foreach ($v as")
    );
    let mut labels = Vec::new();
    let mut rust = Vec::new();
    for document in [
        "{}",
        "{\"\":1,\"0\":2,\"-0\":3,\"01\":4}",
        "{\"a\":{},\"b\":[{},{}],\"a\":2}",
        "{\"\\u0000\":1}",
        "{\"\\u0000\":}",
        "{\"\\u0000\":[[]]}",
        "[{}]",
        "{\"é\":\"😀\"}",
    ] {
        for depth in 1..=5 {
            for associative in [false, true] {
                let expression = format!(
                    "json_decode({}, {}, {depth})",
                    oracle::php_literal(&Value::from(document)),
                    if associative { "true" } else { "false" }
                );
                script.push_str(&format!(
                    "run('case',fn()=> [{expression},json_last_error()]);\n"
                ));
                let flags = Flags {
                    associative,
                    ..Flags::default()
                };
                let result = parse(document.as_bytes(), depth, flags, &mut factory);
                let code = result.as_ref().err().map_or(0, |error| *error as i64);
                let mut values = PhpArray::new();
                values.push(result.unwrap_or(Value::Null)).unwrap();
                values.push(Value::Int(code)).unwrap();
                rust.push(format!("case={}", oracle::encode(&Value::from(values))));
                labels.push(expression);
            }
        }
    }
    let php = oracle::run_php("json-objects", &script).unwrap();
    oracle::compare("object parser", &labels, &php, &rust);
}
