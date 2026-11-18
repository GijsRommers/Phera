//! The procedural functions that draw from the request's Mt19937, after
//! seeding it in both modes, and the properties of `uniqid()` and
//! `lcg_value()`, whose output cannot be compared.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{ArrayKey, NativeFn, PhpArray, Value, prepare_arguments};

use crate::support::{Comparison, php_arguments, registry};

#[test]
fn shuffle_and_keys() {
    let registry = registry();
    let mut context = TestContext::default();
    let mut comparison = Comparison::new();
    for mode in [0, 1] {
        for seed in [-1, 0, 42, i64::MAX] {
            let arguments = vec![Value::Int(seed), Value::Int(mode)];
            call(&registry, &mut context, "mt_srand", arguments).unwrap();
            // Seeding is setup here; its deprecation is compared in methods.rs.
            context.diagnostics.entries.clear();
            comparison.setup(&format!("@mt_srand({seed}, {mode});"));
            for count in 0..=20 {
                let mut array = PhpArray::new();
                for i in 0..count {
                    array.push(Value::Int(i)).unwrap();
                }
                let literal = oracle::php_literal(&Value::from(array.clone()));

                let variable = Rc::new(RefCell::new(Value::from(array.clone())));
                let reference = vec![Value::Reference(variable.clone())];
                call(&registry, &mut context, "shuffle", reference).unwrap();
                let shuffled = Ok(variable.borrow().clone());
                let closure = format!("function () {{ $a = {literal}; shuffle($a); return $a; }}");
                comparison.record_closure(&closure, &shuffled, &mut context);

                for (name, arguments) in [
                    (
                        "str_shuffle",
                        vec![Value::from("abcdef".repeat(count as usize))],
                    ),
                    (
                        "array_rand",
                        vec![Value::from(array.clone()), Value::Int(1)],
                    ),
                ] {
                    let expression = format!("{name}({})", php_arguments(&arguments));
                    let result = call(&registry, &mut context, name, arguments);
                    comparison.record(&expression, &result, &mut context);
                }

                // array_rand() picks among the slots of the array, holes included.
                if count > 3 {
                    array.remove(&ArrayKey::Int(1));
                    let result = call(
                        &registry,
                        &mut context,
                        "array_rand",
                        vec![Value::from(array)],
                    );
                    let closure = format!(
                        "function () {{ $a = {literal}; unset($a[1]); return array_rand($a); }}"
                    );
                    comparison.record_closure(&closure, &result, &mut context);
                }
            }
        }
    }
    comparison.compare("random-procedural", "procedural collections");
}

#[tokio::test]
async fn unique_ids_and_lcg_properties() {
    let registry = registry();
    let mut context = TestContext::default();
    let uniqid = registry.get_function("uniqid").unwrap();
    let NativeFn::Async(uniqid_function) = uniqid.handler else {
        panic!("uniqid() is async: it waits for the next microsecond");
    };
    let mut ids = HashSet::new();
    for more_entropy in [false, true] {
        for _ in 0..1000 {
            let mut arguments = vec![Value::from("prefix"), Value::Bool(more_entropy)];
            prepare_arguments(uniqid, &mut arguments, &mut context).unwrap();
            let Ok(Value::String(id)) = uniqid_function(&mut context, arguments).await else {
                panic!("uniqid() returns a string");
            };
            let id = id.as_bytes();
            assert!(id.starts_with(b"prefix"));
            // 13 hexadecimal digits, then 10 characters of entropy if asked.
            assert_eq!(id.len(), if more_entropy { 29 } else { 19 });
            assert!(ids.insert(id.to_vec()), "uniqid() repeated itself");
        }
    }
    for _ in 0..1000 {
        let Ok(Value::Float(value)) = call(&registry, &mut context, "lcg_value", vec![]) else {
            panic!("lcg_value() returns a float");
        };
        assert!(value > 0.0 && value < 1.0);
    }
    // Every call is deprecated; PHP agrees on the message.
    assert_eq!(context.diagnostics.entries.len(), 1000);
    context.diagnostics.entries.truncate(1);
    let mut comparison = Comparison::new();
    let in_range = Ok(Value::Bool(true));
    comparison.record_closure(
        "function () { $x = lcg_value(); return $x > 0 && $x < 1; }",
        &in_range,
        &mut context,
    );
    comparison.compare("random-lcg-deprecation", "lcg_value() deprecation");
}
