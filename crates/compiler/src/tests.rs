//! Compiler entry-point and name-resolution regression checks.

use phera_bytecode::Reg;

use super::compile;

#[test]
fn tiny_caller_stack() {
    std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(|| {
            let compiler = compile("<?php echo 42;", "test.php").expect("valid PHP");
            assert!(compiler.main.dynamic_scope);
        })
        .expect("thread")
        .join()
        .expect("join");
}

#[test]
fn namespace_fallback_and_import() {
    let compiler = compile(
        "<?php namespace App; use function Util\\f as g; g();f();",
        "test.php",
    )
    .expect("valid PHP");
    assert!(
        compiler
            .symbols
            .iter()
            .any(|symbol| symbol.name == "util\\f")
    );
    assert!(
        compiler
            .symbols
            .iter()
            .any(|symbol| symbol.name == "app\\f" && symbol.fallback.as_deref() == Some("f"))
    );
}

#[test]
fn locals_precede_temporaries() {
    let compiler = compile("<?php $a=1;$b=2;echo $a+$b;", "test.php").expect("valid PHP");
    assert_eq!(
        compiler
            .main
            .locals
            .iter()
            .map(|name| compiler.names[name.index()].as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert!(matches!(
        compiler.main.instructions.get(1),
        Some(phera_bytecode::Instruction::Assign { local: Reg(0), .. })
    ));
}

/// The public listing must expose property steps and static roots in one path vocabulary.
#[test]
fn class_paths_have_a_golden_listing() {
    let source = "<?php class Counter {public $value=0;public static $total=0;function tick(){$this->value=1;self::$total++;}}\n";
    let unit = compile(source, "class-paths.php").unwrap();
    assert_eq!(
        unit.to_string(),
        include_str!("../../../tests/fixtures/classes/m3b-path-listing.txt")
    );
}

/// Captures, arrows and first-class callables keep a readable, stable bytecode contract.
#[test]
fn closures_have_a_golden_listing() {
    let source = include_str!("../../../tests/fixtures/closures/bytecode.php");
    let unit = compile(source, "closure-listing.php").unwrap();
    assert_eq!(
        unit.to_string(),
        include_str!("../../../tests/fixtures/closures/bytecode.txt")
    );
}
