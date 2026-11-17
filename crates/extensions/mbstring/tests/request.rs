//! By-reference results and independent requests are part of the native contract.
use phera_ext_mbstring::Mbstring;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, PhpArray, Registry, Value};
use std::{cell::RefCell, rc::Rc};
fn pair(a: Value, b: Value) -> Value {
    let mut array = PhpArray::new();
    array.push(a).unwrap();
    array.push(b).unwrap();
    Value::from(array)
}
#[test]
fn reference_outputs() {
    let mut registry = Registry::new();
    Mbstring.register(&mut registry);
    let mut script = format!("<?php\n{}", oracle::PHP_PREAMBLE);
    let mut rust = Vec::new();
    for input in [
        "",
        "a=1&b=2",
        "a[]=1&a[]=2",
        "x[a][b]=é",
        "a.b=one+two",
        "x=%FF",
        "a=1&a=2",
    ] {
        let mut ctx = TestContext::default();
        let cell = Rc::new(RefCell::new(Value::Null));
        let value = Value::from(input);
        let result = call(
            &registry,
            &mut ctx,
            "mb_parse_str",
            vec![value.clone(), Value::Reference(cell.clone())],
        )
        .map(|v| pair(v, cell.borrow().clone()));
        script.push_str(&format!(
            "run('ref', function() {{ $r=null; $ok=mb_parse_str({}, $r); return [$ok,$r]; }});\n",
            oracle::php_literal(&value)
        ));
        rust.push(oracle::encode_case(
            "ref",
            &result,
            &ctx.output,
            &ctx.diagnostics,
        ));
    }
    for input in [Value::from("é"), pair(Value::from("é"), Value::from("😀"))] {
        let mut ctx = TestContext::default();
        let cell = Rc::new(RefCell::new(input.clone()));
        let result = call(
            &registry,
            &mut ctx,
            "mb_convert_variables",
            vec![
                Value::from("ASCII"),
                Value::from("UTF-8"),
                Value::Reference(cell.clone()),
            ],
        )
        .map(|v| pair(v, cell.borrow().clone()));
        script.push_str(&format!(
            "run('ref', function() {{ $v={}; \
             $r=mb_convert_variables('ASCII','UTF-8',$v); return [$r,$v]; }});\n",
            oracle::php_literal(&input)
        ));
        rust.push(oracle::encode_case(
            "ref",
            &result,
            &ctx.output,
            &ctx.diagnostics,
        ));
    }
    let php = oracle::run_php("mbstring-references", &script).unwrap();
    oracle::compare("reference outputs", &[], &php, &rust);
}
#[test]
fn settings_are_request_local() {
    let mut registry = Registry::new();
    Mbstring.register(&mut registry);
    let mut first = TestContext::default();
    let mut second = TestContext::default();
    for (setter, value, expected) in [
        (
            "mb_internal_encoding",
            Value::from("ASCII"),
            Value::from("UTF-8"),
        ),
        (
            "mb_substitute_character",
            Value::Int(0xfffd),
            Value::Int(63),
        ),
        (
            "mb_language",
            Value::from("Japanese"),
            Value::from("neutral"),
        ),
    ] {
        call(&registry, &mut first, setter, vec![value]).unwrap();
        assert_eq!(
            call(&registry, &mut second, setter, vec![]).unwrap(),
            expected
        );
    }
}
