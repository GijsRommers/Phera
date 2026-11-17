//! Walk state, cycles, mutation safety, and callbacks that actually suspend.

mod callback_support;

use std::cell::RefCell;
use std::rc::Rc;
use std::task::{Context as TaskContext, Poll, Waker};

use callback_support::{Harness, PHP_CALLBACKS, list, ready};
use phera_extension_api::testing::oracle;
use phera_extension_api::{Context, NativeError, Value};

#[test]
fn walk_objects_and_mutation_match_php() {
    let mut script = format!("<?php\n{}\n{PHP_CALLBACKS}", oracle::PHP_PREAMBLE);
    let mut labels = Vec::new();
    let mut rust = Vec::new();
    for name in ["array_walk", "array_walk_recursive"] {
        for object in [false, true] {
            for extra in [None, Some(Value::Null), Some(Value::Int(8))] {
                for replacement in [None, Some(Value::Int(4))] {
                    let mut context = Harness {
                        mutate: replacement.is_some(),
                        replacement: replacement.clone(),
                        ..Harness::default()
                    };
                    let array = list([
                        Value::Int(1),
                        list([Value::Int(2), Value::Int(3)]),
                        Value::Int(4),
                    ]);
                    let input = if object {
                        let Value::Array(properties) = &array else {
                            panic!("array");
                        };
                        context
                            .create_object("stdClass", (**properties).clone())
                            .expect("object")
                    } else {
                        array.clone()
                    };
                    let target = Value::Reference(Rc::new(RefCell::new(input)));
                    context.target = Some(target.clone());
                    let mut arguments = vec![target.clone(), Value::from("probe_walk")];
                    arguments.extend(extra.clone());
                    let result = ready(context.invoke(name, arguments));
                    let outcome = match result {
                        Ok(value) => list([Value::Bool(true), value]),
                        Err(NativeError::Throw { class, message, .. }) => {
                            list([Value::Bool(false), Value::from(class), Value::from(message)])
                        }
                        Err(error) => panic!("unexpected {error:?}"),
                    };
                    let result = Ok(list([outcome, target.deref().clone(), list(context.trace)]));
                    rust.push(oracle::encode_case(
                        name,
                        &result,
                        &context.base.output,
                        &context.base.diagnostics,
                    ));
                    let third = extra.as_ref().map_or(String::new(), |value| {
                        format!(",{}", oracle::php_literal(value))
                    });
                    let cast = if object { "(object)" } else { "" };
                    let literal = replacement
                        .as_ref()
                        .map_or("null".into(), oracle::php_literal);
                    labels.push(format!(
                        "{name}: object={object}, extra={third}, replace={literal}"
                    ));
                    script.push_str(&format!(
                        "$TRACE=[];$CALLS=0;$MUTATE={};\n\
                        $REPLACEMENT={literal};$TARGET={cast}{};\n\
                        run('{name}', function() {{global $TARGET,$TRACE;\n\
                        try {{$r=[true,{name}($TARGET,'probe_walk'{third})];}}\n\
                        catch(Throwable $e) {{$r=[false,get_class($e),$e->getMessage()];}}\n\
                        return [$r,$TARGET,$TRACE];}});\n",
                        replacement.is_some(),
                        oracle::php_literal(&array)
                    ));
                }
            }
        }
    }
    let php = oracle::run_php("callback_walk_state", &script).expect("PHP required");
    oracle::compare("walk state", &labels, &php, &rust);
}

#[test]
fn recursive_walk_detects_reference_cycles() {
    let target = Rc::new(RefCell::new(Value::Null));
    *target.borrow_mut() = list([Value::Reference(target.clone())]);
    let mut context = Harness::default();
    let result = ready(context.invoke(
        "array_walk_recursive",
        vec![Value::Reference(target.clone()), Value::from("probe_walk")],
    ));
    // Break the test's cycle after walking, including on assertion failure.
    *target.borrow_mut() = Value::Null;
    let rust = oracle::encode_case("cycle", &result, &[], &context.base.diagnostics);
    let script = format!(
        "<?php\n{}\n{PHP_CALLBACKS}\n\
        run('cycle', function() {{$a=[];$a[]=&$a;return array_walk_recursive($a,'probe_walk');}});",
        oracle::PHP_PREAMBLE
    );
    let php = oracle::run_php("callback_walk_cycle", &script).expect("PHP required");
    oracle::compare("walk cycle", &["self reference".into()], &php, &[rust]);
}

#[test]
fn appending_walk_callback_has_a_bounded_visit_set() {
    let target = Value::Reference(Rc::new(RefCell::new(list([Value::Int(1), Value::Int(2)]))));
    let mut context = Harness {
        target: Some(target.clone()),
        ..Harness::default()
    };
    let result = ready(context.invoke(
        "array_walk",
        vec![target.clone(), Value::from("probe_append")],
    ));
    assert_eq!(result.expect("walk"), Value::Bool(true));
    assert_eq!(context.calls, 2);
    let value = target.deref();
    let Value::Array(array) = &*value else {
        panic!("array");
    };
    assert_eq!(array.len(), 4);
}

#[test]
fn callbacks_can_suspend_without_holding_array_borrows() {
    let target = Value::Reference(Rc::new(RefCell::new(list([Value::Int(2), Value::Int(1)]))));
    for (name, callback) in [("array_walk", "probe_walk"), ("usort", "probe_compare")] {
        let mut context = Harness {
            yield_callbacks: true,
            ..Harness::default()
        };
        let mut future = context.invoke(name, vec![target.clone(), Value::from(callback)]);
        let mut task = TaskContext::from_waker(Waker::noop());
        let mut pending = 0;
        loop {
            match future.as_mut().poll(&mut task) {
                Poll::Pending => {
                    pending += 1;
                    assert!(pending < 10);
                    let Value::Reference(cell) = &target else {
                        panic!("reference");
                    };
                    assert!(cell.try_borrow_mut().is_ok(), "borrow held across callback");
                }
                Poll::Ready(result) => {
                    assert!(result.is_ok());
                    break;
                }
            }
        }
        assert!(pending > 0);
    }
}
