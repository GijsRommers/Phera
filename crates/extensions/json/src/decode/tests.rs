//! Object-mode decoding against PHP. The parser is called directly with a
//! factory that makes test stdClass objects, because the test context of
//! the extension API has no real classes.
use super::parser::parse;
use crate::flags::DecodeFlags;
use phera_extension_api::testing::oracle;
use phera_extension_api::value::ClassHandle;
use phera_extension_api::{Object, PhpArray, Value};
use std::{any::Any, rc::Rc};

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
        object.set(key, value.clone());
    }
    Value::Object(Rc::new(object))
}

#[test]
fn object_parser_matches_php() {
    // The preamble's enc() casts an object to an array, which in PHP turns
    // the property "0" into the key 0, while the Rust side encodes property
    // names as stored. A foreach over the object shows them as stored.
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
                let flags = DecodeFlags {
                    associative,
                    ..DecodeFlags::default()
                };
                let result = parse(document.as_bytes(), depth, flags, &mut factory);
                let code = result.as_ref().err().map_or(0, |error| error.code() as i64);
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
