//! Stress and native-boundary tests exercise exception ownership on heap frames.

use crate::Vm;
use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry, Value,
};
use std::sync::Arc;

/// Native Throwable entries are the same ones linked by the CLI runtime.
fn registry() -> Registry {
    let mut registry = Registry::new();
    registry.class(phera_extension_api::ClassEntry::interface("Throwable"));
    registry.class(phera_extension_api::ClassEntry {
        interfaces: &["Throwable"],
        ..phera_extension_api::ClassEntry::class("Exception")
    });
    registry
}

/// A hundred thousand PHP calls must unwind in a loop without growing Rust's stack.
#[tokio::test(flavor = "current_thread")]
async fn a_hundred_thousand_frames_unwind_iteratively() {
    let unit = phera_compiler::compile("<?php function descend($depth){if($depth==0)throw new Exception('deep');descend($depth-1);}try{descend(100000);}catch(Throwable $error){echo 'deep';}", "deep.php").unwrap();
    let mut interpreter = Vm::new(Arc::new(registry()));
    assert_eq!(interpreter.run(Arc::new(unit)).await, 0);
    assert_eq!(interpreter.output, b"deep");
    assert_eq!(interpreter.call_stack.depth(), 0);
    assert_eq!(interpreter.registers.len(), 0);
    assert!(interpreter.finally_stack.is_empty());
    assert_eq!(interpreter.call_stack.retained_exception_values(), 0);
}

/// Sequential throws reuse frames and release every argument and temporary owner.
#[tokio::test(flavor = "current_thread")]
async fn a_million_throws_do_not_grow_live_vm_storage() {
    let unit = phera_compiler::compile("<?php function fail(){throw new Exception('x');}$caught=0;for($index=0;$index<1000000;$index++){try{fail();}catch(Throwable){$caught++;}}echo $caught;", "million.php").unwrap();
    let mut interpreter = Vm::new(Arc::new(registry()));
    assert_eq!(interpreter.run(Arc::new(unit)).await, 0);
    assert_eq!(interpreter.output, b"1000000");
    assert_eq!(interpreter.call_stack.depth(), 0);
    assert_eq!(interpreter.registers.len(), 0);
    assert!(interpreter.finally_stack.is_empty());
    assert_eq!(interpreter.call_stack.retained_exception_values(), 0);
    assert!(interpreter.call_stack.recycled_count() <= 2);
    assert!(interpreter.classes.objects.len() <= 2);
}

/// The extension forwards NativeError::Unwind unchanged through its await point.
fn native_callback<'context>(
    context: &'context mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'context, NativeResult> {
    Box::pin(async move {
        let callable = arguments.first().cloned().unwrap_or_default();
        context.call(&callable, vec![Value::Int(7)]).await
    })
}

/// A callback exception must leave the native invocation and reach the PHP caller's catch.
#[tokio::test(flavor = "current_thread")]
async fn exceptions_cross_a_native_callback() {
    let mut registry = registry();
    const PARAMETERS: &[Param] = &[Param::new("callback", ParamType::Mixed)];
    registry.function(FunctionEntry::asynchronous(
        "native_callback",
        PARAMETERS,
        native_callback,
    ));
    let unit = phera_compiler::compile("<?php function fail($number){throw new Exception('native');}try{native_callback('fail');}catch(Throwable $error){echo 'native';}echo ' resumed';", "native.php").unwrap();
    let mut interpreter = Vm::new(Arc::new(registry));
    assert_eq!(interpreter.run(Arc::new(unit)).await, 0);
    assert_eq!(interpreter.output, b"native resumed");
    assert_eq!(interpreter.registers.len(), 0);
    assert!(interpreter.finally_stack.is_empty());
    assert_eq!(interpreter.call_stack.retained_exception_values(), 0);
}

/// Native object creation links classes, bypasses constructors and rejects unknown names.
#[test]
fn native_object_creation_uses_the_vm_class_table() {
    let mut interpreter = Vm::new(Arc::new(registry()));
    let mut properties = phera_value::PhpArray::new();
    properties.insert(phera_value::ArrayKey::str("number"), Value::Int(7));
    let Value::Object(object) = interpreter.create_object("stdClass", properties).unwrap() else {
        panic!("object expected");
    };
    assert_eq!(
        object.get(&phera_value::ArrayKey::str("number")),
        Some(Value::Int(7))
    );
    let error = interpreter
        .create_object("Absent", phera_value::PhpArray::new())
        .unwrap_err();
    assert!(matches!(
        error,
        phera_extension_api::NativeError::Throw { class: "Error", .. }
    ));
}
