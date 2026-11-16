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
        "<?php class Example {static function method(){}} Example::method(...);",
        "<?php class Example {function method(){}} (new Example)->method(...);",
        "<?php class Example {public int $value {get => 1;}}",
        "<?php class Example {public private(set) int $value;}",
        "<?php #[Other] class Example {}",
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

/// Forwarded calls retain both the receiver and the called class across three levels.
#[tokio::test]
async fn static_style_calls_forward_receiver_and_called_class() {
    let source = "<?php class Root { public $value; function __construct($value){$this->value=$value;} static function make(){return new static;} function label(){return 'root';}} class Middle extends Root {function __construct(){parent::__construct(9);} function label(){return parent::label();} static function build(){return parent::make();}} class Leaf extends Middle {} $name='Leaf'; $object=new $name; $method='label';echo $object->value,$object->$method(),Leaf::build()::class; echo $object instanceof $name ? 'y':'n';";
    let unit = phera_compiler::compile(source, "forwarding.php").unwrap();
    let mut interpreter = Vm::new(Arc::new(Registry::new()));
    assert_eq!(interpreter.run(Arc::new(unit)).await, 0);
    assert_eq!(interpreter.output, b"9rootLeafy");
}
