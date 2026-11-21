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

/// `self::K` / `parent::K` in a method of a named class compile to the named
/// form with the resolved class, like `C::K`; in a closure, an arrow function,
/// a trait, or a class without a parent they keep the general register form.
#[test]
fn relative_class_constants_in_methods_use_the_named_form() {
    let source = "<?php class P { const K = 1; }
        trait T { const K = 3; function t() { return self::K; } }
        class C extends P { const K = 2;
            function m() { return self::K + parent::K + static::K; }
            function c() { return function () { return self::K; }; }
            function a() { return fn () => parent::K; } }
        class N { function f() { return parent::K; } }";
    let unit = compile(source, "self-constants.php").expect("valid PHP");
    let class = unit.classes.iter().find(|c| c.name == "C").expect("C");
    let method = |name: &str| {
        let method = class
            .methods
            .iter()
            .find(|m| unit.names[m.name.index()] == name)
            .unwrap_or_else(|| panic!("method {name}"));
        &unit.functions[method.function.index()]
    };
    let named: Vec<&str> = method("m")
        .instructions
        .iter()
        .filter_map(|i| match i {
            phera_bytecode::Instruction::FetchNamedClassConstant { class, .. } => {
                Some(unit.symbols[class.index()].name.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(named, ["C", "P"], "self then parent use the resolved class");
    // `static::K` keeps the general register form.
    assert!(
        method("m")
            .instructions
            .iter()
            .any(|i| matches!(i, phera_bytecode::Instruction::FetchClassConstant { .. }))
    );
    // Closures, arrow functions, trait methods and a parentless class keep it too.
    for function in &unit.functions {
        let general = function
            .instructions
            .iter()
            .any(|i| matches!(i, phera_bytecode::Instruction::FetchClassConstant { .. }));
        let named = function.instructions.iter().any(|i| {
            matches!(
                i,
                phera_bytecode::Instruction::FetchNamedClassConstant { .. }
            )
        });
        let keep_general = function.name.starts_with("{closure:")
            || function.name == "T::t"
            || function.name == "N::f";
        if keep_general {
            assert!(
                general && !named,
                "{} keeps the general form",
                function.name
            );
        }
    }
}
