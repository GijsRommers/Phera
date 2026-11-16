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
