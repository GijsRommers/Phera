//! Closure lifetime tests exercise heap calls: `$f = function () use (&$f) { $f(); };`.

use std::sync::Arc;

use phera_extension_api::{ClassEntry, Context, MethodEntry, NativeResult, Registry};
use phera_value::{Object, Value};
use std::rc::Rc;

use crate::Vm;

fn error_message(
    _context: &mut dyn Context,
    object: Option<&Rc<Object>>,
    _arguments: &[Value],
) -> NativeResult {
    Ok(object
        .and_then(|object| object.get(&Object::protected_key("message")))
        .unwrap_or_default())
}

fn registry() -> Registry {
    let mut registry = Registry::new();
    registry.class(ClassEntry::interface("Throwable"));
    registry.class(ClassEntry {
        interfaces: &["Throwable"],
        methods: const { &[MethodEntry::public("getMessage", &[], error_message)] },
        ..ClassEntry::class("Error")
    });
    registry
}

#[tokio::test(flavor = "current_thread")]
async fn a_million_closures_reuse_frames_registers_and_object_ids() {
    let source = r#"<?php
        $result = 0;
        for ($index = 0; $index < 1000000; ++$index) {
            $closure = function ($value) use ($index) { $sum = $value + $index; return $sum; };
            $result = $closure(1);
        }
        echo $result;
    "#;
    let unit = phera_compiler::compile(source, "million-closures.php").unwrap();
    let mut vm = Vm::new(Arc::new(registry()));
    assert_eq!(vm.run(Arc::new(unit)).await, 0);
    assert_eq!(vm.output, b"1000000");
    assert_eq!(vm.call_stack.depth(), 0);
    assert_eq!(vm.registers.len(), 0);
    assert!(vm.call_stack.recycled_count() <= 2);
    assert!(vm.classes.objects.len() <= 3);
    assert!(vm.closure_captures.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn a_self_capturing_closure_is_released_at_request_shutdown() {
    let unit = phera_compiler::compile(
        "<?php $f = function () use (&$f) { return $f; };",
        "self-capture.php",
    )
    .unwrap();
    let mut vm = Vm::new(Arc::new(registry()));
    assert_eq!(vm.run(Arc::new(unit)).await, 0);
    let closure = vm.classes.objects[0].clone();
    assert!(closure.upgrade().is_some());
    drop(vm);
    assert!(closure.upgrade().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn a_hundred_thousand_closure_calls_hit_the_heap_frame_limit() {
    let source = r#"<?php
        $recurse = function ($depth) use (&$recurse) {
            if ($depth === 0) return 0;
            return $recurse($depth - 1);
        };
        try { $recurse(100000); }
        catch (Error $error) { echo $error->getMessage(); }
    "#;
    let unit = phera_compiler::compile(source, "closure-depth.php").unwrap();
    let mut vm = Vm::new(Arc::new(registry()));
    vm.frame_limit = 100000;
    assert_eq!(vm.run(Arc::new(unit)).await, 0);
    assert_eq!(vm.output, b"Maximum call stack depth of 100000 exceeded");
    assert_eq!(vm.call_stack.depth(), 0);
    assert_eq!(vm.registers.len(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn deep_capture_chains_drop_without_rust_recursion() {
    let source = r#"<?php
        $head = null;
        for ($index = 0; $index < 100000; ++$index) {
            $head = fn() => $head;
        }
        unset($head);
        echo 'released';
    "#;
    let unit = phera_compiler::compile(source, "capture-chain.php").unwrap();
    let mut vm = Vm::new(Arc::new(registry()));
    assert_eq!(vm.run(Arc::new(unit)).await, 0);
    assert_eq!(vm.output, b"released");
    assert!(
        vm.classes
            .objects
            .iter()
            .all(|object| object.upgrade().is_none())
    );
}
