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
        "<?php $callback = function () {};",
        "<?php $callback = fn () => 1;",
        "<?php class Example {static function method(){}} Example::method(...);",
        "<?php class Example {function method(){}} (new Example)->method(...);",
    ] {
        if matches!(
            source,
            "<?php $callback = function () {};"
                | "<?php $callback = fn () => 1;"
                | "<?php class Example {static function method(){}} Example::method(...);"
                | "<?php class Example {function method(){}} (new Example)->method(...);"
        ) {
            let unit = phera_compiler::compile(source, "supported.php").unwrap();
            assert!(unit.main.instructions.iter().any(|instruction| matches!(
                instruction,
                phera_bytecode::Instruction::MakeClosure { .. }
                    | phera_bytecode::Instruction::MakeCallable { .. }
            )));
            continue;
        }
        let error = phera_compiler::compile(source, "unsupported.php").unwrap_err();
        assert!(
            error.message.contains("not supported yet"),
            "{source}: {error}"
        );
    }
}

#[test]
fn property_hooks_and_set_visibility_emit_access_metadata() {
    let unit = phera_compiler::compile(
        "<?php class Example {public int $value {get => 1;} public private(set) int $stored;}",
        "supported-properties.php",
    )
    .unwrap();
    let properties = &unit.classes[0].properties;
    let hooked = properties[0].hooks.as_ref().unwrap();
    assert!(hooked.is_virtual);
    let getter = hooked.get.as_ref().unwrap();
    assert_eq!(
        unit.functions[getter.function.index()].name,
        "Example::$value::get"
    );
    assert!(hooked.set.is_none());

    assert!(properties[1].hooks.is_none());
    assert_eq!(
        properties[1].set_visibility,
        Some(phera_bytecode::Visibility::Private)
    );
    assert!(properties[1].is_final);
}

#[test]
fn generator_body_starts_with_a_suspension_boundary() {
    let unit = phera_compiler::compile("<?php function generator() { yield 1; }", "generator.php")
        .unwrap();
    assert!(matches!(
        unit.functions.as_slice(),
        [function] if function.is_generator
            && matches!(function.instructions.first(), Some(phera_bytecode::Instruction::GeneratorStart))
    ));
}

#[test]
fn class_declaration_retains_attributes_and_set_visibility() {
    let unit = phera_compiler::compile(
        "<?php #[Other] class Example {public private(set) int $value;}",
        "supported.php",
    )
    .unwrap();
    let class = &unit.classes[0];
    assert_eq!(class.metadata.attributes[0].name, "Other");
    assert_eq!(
        class.properties[0].set_visibility,
        Some(phera_bytecode::Visibility::Private)
    );
}

#[test]
fn goto_uses_an_existing_jump_instruction() {
    let unit = phera_compiler::compile("<?php goto target; target:", "goto.php").unwrap();
    assert!(matches!(
        unit.main.instructions.first(),
        Some(phera_bytecode::Instruction::Jump { target }) if target.index() == 1
    ));
}

#[test]
fn class_forms_emit_their_declaration_kind() {
    use phera_bytecode::{ClassKind, Instruction};

    for (source, kind) in [
        ("<?php trait Example {}", ClassKind::Trait),
        ("<?php enum Example {}", ClassKind::Enum),
        ("<?php new class {};", ClassKind::Anonymous),
    ] {
        let unit = phera_compiler::compile(source, "supported.php").unwrap();
        assert_eq!(unit.classes.len(), 1);
        assert_eq!(unit.classes[0].kind, kind);
        assert!(
            unit.main
                .instructions
                .iter()
                .any(|instruction| matches!(instruction, Instruction::DeclareClass { .. }))
        );
    }
}

#[test]
fn eval_emits_a_scoped_unit_instruction() {
    let unit = phera_compiler::compile("<?php eval('echo 1;');", "supported.php").unwrap();
    assert!(
        unit.main
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, phera_bytecode::Instruction::Eval { .. }))
    );
}

#[tokio::test]
async fn clone_invokes_magic_on_the_copy() {
    let source = "<?php class Example { public $value = 7; function __clone() { ++$this->value; } } $original = new Example; $copy = clone $original; echo $original->value, ':', $copy->value;";
    let unit = phera_compiler::compile(source, "clone.php").unwrap();
    let mut interpreter = Vm::new(Arc::new(Registry::new()));
    assert_eq!(interpreter.run(Arc::new(unit)).await, 0);
    assert_eq!(interpreter.output, b"7:8");
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

/// Request-local site caches must never reuse classes from a previous VM.
#[tokio::test]
async fn conditional_classes_are_isolated_with_shared_bytecode() {
    let unit = Arc::new(phera_compiler::compile(
        "<?php if (true) { class LocalClass { public int $value = 1; function read() { return $this->value; } } } $object = new LocalClass; echo $object->read(); $object->value++; echo $object->read();",
        "conditional-cache.php",
    ).unwrap());
    let registry = Arc::new(Registry::new());
    for _ in 0..3 {
        let mut interpreter = Vm::new(registry.clone());
        assert_eq!(interpreter.run(unit.clone()).await, 0);
        assert_eq!(interpreter.output, b"12");
    }
}

/// A cached accepted class must not accept another class at the same TypeId.
#[tokio::test]
async fn class_type_caches_guard_parameter_and_return_receiver_identity() {
    for parameter in ["Contract $value", "$value"] {
        let source = format!(
            "<?php interface Contract {{}} class A implements Contract {{}} class B implements Contract {{}} class C {{}} function checked({parameter}): Contract {{return $value;}} checked(new A); checked(new B); checked(new A); echo 'checked'; checked(new C);"
        );
        let unit = Arc::new(phera_compiler::compile(&source, "typed-cache.php").unwrap());
        let mut interpreter = Vm::new(Arc::new(Registry::new()));
        assert_ne!(interpreter.run(unit).await, 0);
        assert!(interpreter.output.starts_with(b"checked"));
        assert!(String::from_utf8_lossy(&interpreter.output).contains("must be of type Contract"));
    }
}

/// Leaf execution must preserve the public heap-frame limit even when no frame is allocated.
#[tokio::test]
async fn integer_leaf_calls_still_obey_the_frame_limit() {
    for source in [
        "<?php class Leaf {function get(){return 1;}} $object=new Leaf;echo $object->get();",
        "<?php class Leaf {static function get(){return 1;}} echo Leaf::get();",
        "<?php class Leaf {public int $number=1;function get(){return $this->number;}} $object=new Leaf;echo $object->get();",
    ] {
        let unit = Arc::new(phera_compiler::compile(source, "leaf-limit.php").unwrap());
        let mut interpreter = Vm::new(Arc::new(Registry::new()));
        interpreter.frame_limit = 1;
        assert_ne!(interpreter.run(unit).await, 0);
        assert!(
            String::from_utf8_lossy(&interpreter.output)
                .contains("Maximum call stack depth of 1 exceeded")
        );
    }
}
