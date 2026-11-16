//! Class calls exercise heap frames rather than recursive Rust interpreter calls.

use std::sync::Arc;

use phera_extension_api::Registry;
use phera_vm::Vm;

#[tokio::test]
async fn a_hundred_thousand_method_calls_use_heap_frames() {
    let source = "<?php class Counter { function down($number) { if ($number == 0) return 7; return $this->down($number - 1); } } echo (new Counter)->down(100000);";
    let unit = phera_compiler::compile(source, "deep-method.php").unwrap();
    let mut interpreter = Vm::new(Arc::new(Registry::new()));
    assert_eq!(interpreter.run(Arc::new(unit)).await, 0);
    assert_eq!(interpreter.output, b"7");
}

#[test]
fn later_milestones_remain_compile_errors() {
    for source in [
        "<?php try {} catch (Exception $error) {}",
        "<?php throw new Exception;",
        "<?php $callback = function () {};",
        "<?php $callback = fn () => 1;",
        "<?php trait Example {}",
        "<?php enum Example {}",
        "<?php new class {};",
        "<?php function generator() { yield 1; }",
        "<?php goto target; target:",
        "<?php eval('echo 1;');",
    ] {
        let error = phera_compiler::compile(source, "unsupported.php").unwrap_err();
        assert!(
            error.message.contains("not supported yet"),
            "{source}: {error}"
        );
    }
}

#[tokio::test]
async fn clone_magic_is_not_implicitly_invoked_in_this_milestone() {
    let source = "<?php class Example { function __clone() { echo 'later'; } } $original = new Example; $copy = clone $original; echo 7;";
    let unit = phera_compiler::compile(source, "clone.php").unwrap();
    let mut interpreter = Vm::new(Arc::new(Registry::new()));
    assert_eq!(interpreter.run(Arc::new(unit)).await, 0);
    assert_eq!(interpreter.output, b"7");
}
