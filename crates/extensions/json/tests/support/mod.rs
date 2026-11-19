//! Shared differential harness and setup-dependent encoder cases.
use phera_ext_json::Json;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, PhpArray, Registry, Value};
/// Deterministic xorshift samples cover flags, IEEE bit patterns and mutations.
pub fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}
pub fn list(values: Vec<Value>) -> Value {
    let mut array = PhpArray::new();
    for value in values {
        array.push(value).unwrap();
    }
    Value::from(array)
}
/// Each observation includes last error, and preserves state across calls.
pub struct Matrix {
    pub registry: Registry,
    pub context: TestContext,
    pub script: String,
    pub labels: Vec<String>,
    pub rust: Vec<String>,
}
impl Matrix {
    pub fn new() -> Self {
        let mut registry = Registry::for_level(phera_extension_api::testing::oracle::test_level());
        Json.register(&mut registry);
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
        self.expression(function, arguments, expression);
    }
    pub fn expression(&mut self, function: &str, arguments: Vec<Value>, expression: String) {
        self.script.push_str(&format!(
            "run('case', fn() => [{expression}, json_last_error(), json_last_error_msg()]);\n"
        ));
        let result = call(&self.registry, &mut self.context, function, arguments).map(|value| {
            let code = call(&self.registry, &mut self.context, "json_last_error", vec![]).unwrap();
            let message = call(
                &self.registry,
                &mut self.context,
                "json_last_error_msg",
                vec![],
            )
            .unwrap();
            list(vec![value, code, message])
        });
        self.rust.push(oracle::encode_case(
            "case",
            &result,
            &self.context.output,
            &self.context.diagnostics,
        ));
        self.labels.push(expression);
    }
}
/// Public properties, mangled visibility keys and cyclic references need setup code.
pub fn extra_cases(matrix: &mut Matrix) {
    use phera_extension_api::value::ClassHandle;
    use phera_extension_api::{ArrayKey, Object};
    use std::{any::Any, cell::RefCell, rc::Rc};
    struct Class;
    impl ClassHandle for Class {
        fn name(&self) -> &str {
            "stdClass"
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }
    let object = Rc::new(Object::new(1, Rc::new(Class)));
    object.set(ArrayKey::str("a"), Value::Int(1));
    object.set(Object::protected_key("hidden"), Value::Int(2));
    object.set(Object::private_key("Sample", "secret"), Value::Int(3));
    let reference = Rc::new(RefCell::new(Value::Null));
    let cyclic = list(vec![Value::Reference(reference.clone())]);
    *reference.borrow_mut() = cyclic.clone();
    for flags in [0, 16, 128, 512, 4194304, 4194816] {
        for depth in [-1, 0, 1, 2, 512] {
            matrix.expression("json_encode", vec![Value::Object(object.clone()), Value::Int(flags), Value::Int(depth)], format!("json_encode(new class {{ public $a = 1; protected $hidden = 2; private $secret = 3; }}, {flags}, {depth})"));
            matrix.expression("json_encode", vec![cyclic.clone(), Value::Int(flags), Value::Int(depth)], format!("(function() {{ $value=[]; $value[]=&$value; return json_encode($value, {flags}, {depth}); }})()"));
        }
    }
    *reference.borrow_mut() = Value::Null;
    for input in ["", "null", "{}", "[]", "[[]]", "{\"a\":[]}"] {
        for depth in [-1, 0, 1, 2, 3, 2147483648] {
            matrix.case(
                "json_decode",
                vec![Value::from(input), Value::Bool(true), Value::Int(depth)],
            );
            matrix.case("json_validate", vec![Value::from(input), Value::Int(depth)]);
        }
    }
    for flags in [-1, 1, 2, 3, 1048576, 2097152, 4194304] {
        matrix.case(
            "json_validate",
            vec![Value::from("null"), Value::Int(512), Value::Int(flags)],
        );
    }
    let constants: Vec<_> = matrix
        .registry
        .constants()
        .map(|(name, value)| (name.to_string(), value.to_value()))
        .collect();
    for (name, value) in constants {
        matrix
            .script
            .push_str(&format!("run('constant', fn()=> {name});\n"));
        matrix.labels.push(name);
        matrix
            .rust
            .push(format!("constant={}", oracle::encode(&value)));
    }
}

/// Maps cover key normalization, invalid names, and all multibyte byte classes.
pub fn maps_and_bytes(matrix: &mut Matrix) {
    use phera_extension_api::ArrayKey;
    let mut seed = 0x154437abc99;
    for index in 0..4096 {
        let bytes = random(&mut seed).to_le_bytes().to_vec();
        let mut array = PhpArray::new();
        array.insert(ArrayKey::Int(-1), Value::from("1.5"));
        array.insert(ArrayKey::str("01"), Value::from("-0"));
        array.insert(ArrayKey::str(""), Value::from(bytes.clone()));
        array.insert(ArrayKey::str(bytes), Value::Null);
        let flags = [0, 512, 2097152, 1048576, 3145728, 4194304, 32, 128][index % 8];
        matrix.case("json_encode", vec![Value::from(array), Value::Int(flags)]);
    }
}
