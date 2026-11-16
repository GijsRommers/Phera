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

/// Repeated property appends must move the inner array rather than clone its owners.
#[tokio::test]
async fn million_property_appends_preserve_unique_array_ownership() {
    let source = "<?php class Bucket { public $items=[]; function fill() { for ($index=0; $index<1000000; $index++) $this->items[]=$index; return $this->items[999999]; } } echo (new Bucket)->fill();";
    let unit = phera_compiler::compile(source, "property-append.php").unwrap();
    let mut interpreter = Vm::new(Arc::new(Registry::new()));
    assert_eq!(interpreter.run(Arc::new(unit)).await, 0);
    assert_eq!(interpreter.output, b"999999");
}

/// Literal property names remain names in paths rather than runtime string temporaries.
#[test]
fn literal_property_names_are_resolved_in_the_unit_table() {
    let unit = phera_compiler::compile(
        "<?php class Bucket {private $items=[];function write(){$this->items[]=1;}}",
        "keys.php",
    )
    .unwrap();
    let function = unit
        .functions
        .iter()
        .find(|function| function.name == "Bucket::write")
        .unwrap();
    let name = function
        .paths
        .iter()
        .flat_map(|path| &path.steps)
        .find_map(|step| {
            if let phera_bytecode::Step::Property(name) = step {
                Some(*name)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(unit.names[name.index()], "items");
    assert!(function.paths.iter().any(|path| path.steps
        == [
            phera_bytecode::Step::Property(name),
            phera_bytecode::Step::Append
        ]));
}
