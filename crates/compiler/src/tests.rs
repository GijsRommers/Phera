//! Compiler entry-point and name-resolution regression checks.

use phera_bytecode::Reg;

use super::compile;

#[test]
fn scalar_loads_use_immediates_after_folding_literal_readers() {
    use phera_bytecode::{Constant, Instruction as I};

    let unit = compile(
        "<?php function probe() { echo true, false; return null; } $value = false;",
        "test.php",
    )
    .expect("valid PHP");
    let code = &unit.functions[0].instructions;
    assert!(
        code.iter()
            .any(|i| matches!(i, I::LoadBool { value: true, .. }))
    );
    assert!(
        code.iter()
            .any(|i| matches!(i, I::LoadBool { value: false, .. }))
    );
    assert!(code.iter().any(|i| matches!(i, I::LoadNull { .. })));
    assert!(code.iter().all(|i| !matches!(i, I::LoadConst { .. })));
    assert!(unit.main.instructions.iter().any(|i| {
        matches!(i, I::AssignConst { constant, .. }
            if unit.constants[constant.index()] == Constant::Bool(false))
    }));
}

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

/// The instructions of one compiled `f`, for the condition shapes below.
fn function_instructions(source: &str) -> Vec<phera_bytecode::Instruction> {
    let unit = compile(source, "conditions.php").expect("valid PHP");
    unit.functions
        .iter()
        .find(|function| function.name == "f")
        .expect("function f")
        .instructions
        .to_vec()
}

/// How many instructions of `code` match `kind`.
fn count(
    code: &[phera_bytecode::Instruction],
    kind: impl Fn(&phera_bytecode::Instruction) -> bool,
) -> usize {
    code.iter().filter(|instruction| kind(instruction)).count()
}

use phera_bytecode::{BinaryOp, Instruction};

/// `if ($a === $b)` is one compare-and-branch, with no temporary boolean.
#[test]
fn identical_condition_is_one_binary_branch() {
    let code = function_instructions(
        "<?php function f($a, $b) { if ($a === $b) { return 1; } return 0; }",
    );
    assert_eq!(
        count(&code, |i| matches!(
            i,
            Instruction::BinaryBranch {
                op: BinaryOp::Identical,
                ..
            }
        )),
        1
    );
    assert_eq!(count(&code, |i| matches!(i, Instruction::Binary { .. })), 0);
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::JumpIfNot { .. })),
        0
    );
    assert_eq!(count(&code, |i| matches!(i, Instruction::Move { .. })), 0);
}

/// `if ($a !== null)` and `if (null === $a)` reuse the null test itself.
#[test]
fn null_conditions_reuse_the_null_jumps() {
    let code = function_instructions(
        "<?php function f($a) { if ($a !== null) { return 1; } if (null === $a) { return 2; } return 0; }",
    );
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::JumpIfNull { .. })),
        1
    );
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::JumpIfNotNull { .. })),
        1
    );
    // The implicit return still loads null. The two conditions do not.
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::LoadNull { .. })),
        1
    );
    assert_eq!(count(&code, |i| matches!(i, Instruction::Binary { .. })), 0);
}

/// A small integer literal rides inside the branch instruction, on either side.
#[test]
fn integer_literal_conditions_carry_the_literal() {
    let code = function_instructions(
        "<?php function f($a) { if ($a < 10) { return 1; } if (10 > $a) { return 2; } return 0; }",
    );
    let branches: Vec<_> = code
        .iter()
        .filter_map(|i| match i {
            Instruction::BinaryIntBranch { op, lhs, rhs, .. } => Some((*op, *lhs, *rhs)),
            _ => None,
        })
        .collect();
    assert_eq!(
        branches,
        [
            (BinaryOp::Less, phera_bytecode::Reg(0), 10),
            (BinaryOp::Less, phera_bytecode::Reg(0), 10)
        ],
        "`10 > $a` compares as `$a < 10`"
    );
    assert_eq!(count(&code, |i| matches!(i, Instruction::Binary { .. })), 0);
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::JumpIfNot { .. })),
        0
    );
}

/// `if ($a)` and `if (!$a)` branch on the local itself: no Move, no Unary Not.
#[test]
fn plain_local_conditions_jump_on_the_local() {
    let code = function_instructions(
        "<?php function f($a) { if ($a) { return 1; } if (!$a) { return 2; } return 0; }",
    );
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::JumpIfNot { .. })),
        1
    );
    assert_eq!(count(&code, |i| matches!(i, Instruction::JumpIf { .. })), 1);
    assert_eq!(count(&code, |i| matches!(i, Instruction::Move { .. })), 0);
    assert_eq!(count(&code, |i| matches!(i, Instruction::Unary { .. })), 0);
}

/// Loops compile their condition as one branch per round.
#[test]
fn loop_conditions_are_one_branch_per_round() {
    let while_code =
        function_instructions("<?php function f($k, $n) { while ($k < $n) { $k++; } return $k; }");
    assert_eq!(
        count(&while_code, |i| matches!(
            i,
            Instruction::BinaryBranch {
                op: BinaryOp::Less,
                ..
            }
        )),
        1
    );
    assert_eq!(
        count(&while_code, |i| matches!(i, Instruction::Binary { .. })),
        0
    );
    let do_code = function_instructions(
        "<?php function f($k, $n) { do { $k++; } while ($k < $n); return $k; }",
    );
    assert_eq!(
        count(&do_code, |i| matches!(
            i,
            Instruction::BinaryBranchIf {
                op: BinaryOp::Less,
                ..
            }
        )),
        1
    );
    let for_code = function_instructions(
        "<?php function f($k, $n) { for ($k = 0; $k < $n; $k++) { } return $k; }",
    );
    assert_eq!(
        count(&for_code, |i| matches!(
            i,
            Instruction::BinaryBranch {
                op: BinaryOp::Less,
                ..
            }
        )),
        1
    );
    let literal_for = function_instructions(
        "<?php function f($k) { for ($k = 0; $k < 10; $k++) { } return $k; }",
    );
    assert_eq!(
        count(&literal_for, |i| matches!(
            i,
            Instruction::BinaryIntBranch {
                op: BinaryOp::Less,
                rhs: 10,
                ..
            }
        )),
        1
    );
}

/// A ternary's condition branches directly into its two arms.
#[test]
fn ternary_condition_branches_directly() {
    let code = function_instructions("<?php function f($a, $b) { return $a === $b ? 1 : 2; }");
    assert_eq!(
        count(&code, |i| matches!(
            i,
            Instruction::BinaryBranch {
                op: BinaryOp::Identical,
                ..
            }
        )),
        1
    );
    assert_eq!(count(&code, |i| matches!(i, Instruction::Binary { .. })), 0);
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::JumpIfNot { .. })),
        0
    );
}

/// `&&` and `||` in a condition are jumps per operand, with no boolean value.
#[test]
fn short_circuit_conditions_are_plain_jumps() {
    let code = function_instructions(
        "<?php function f($a, $b, $c) { if ($a && $b || $c) { return 1; } return 0; }",
    );
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::JumpIfNot { .. })),
        2
    );
    assert_eq!(count(&code, |i| matches!(i, Instruction::JumpIf { .. })), 1);
    assert_eq!(count(&code, |i| matches!(i, Instruction::Unary { .. })), 0);
    assert_eq!(count(&code, |i| matches!(i, Instruction::Move { .. })), 0);
    assert_eq!(count(&code, |i| matches!(i, Instruction::Binary { .. })), 0);
}

/// A comparison whose value is used still compiles to `Binary`.
#[test]
fn comparison_as_a_value_keeps_binary() {
    let code = function_instructions("<?php function f($a, $b) { return $a === $b; }");
    assert_eq!(
        count(&code, |i| matches!(
            i,
            Instruction::Binary {
                op: BinaryOp::Identical,
                ..
            }
        )),
        1
    );
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::BinaryBranch { .. })),
        0
    );
}

/// `if ($o instanceof C)` is one branch instruction, no boolean temporary.
#[test]
fn instanceof_condition_is_one_branch() {
    let code = function_instructions(
        "<?php class C {} function f($o) { if ($o instanceof C) { return 1; } return 0; }",
    );
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::InstanceOfBranch { .. })),
        1
    );
    assert_eq!(
        count(&code, |i| matches!(
            i,
            Instruction::InstanceOfBranchIf { .. }
        )),
        0
    );
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::InstanceOf { .. })),
        0
    );
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::JumpIfNot { .. })),
        0
    );
    let negated = function_instructions(
        "<?php class C {} function f($o) { if (!($o instanceof C)) { return 1; } return 0; }",
    );
    assert_eq!(
        count(&negated, |i| matches!(
            i,
            Instruction::InstanceOfBranchIf { .. }
        )),
        1
    );
    let dynamic = function_instructions(
        "<?php function f($o, $c) { if ($o instanceof $c) { return 1; } return 0; }",
    );
    assert_eq!(
        count(&dynamic, |i| matches!(
            i,
            Instruction::DynamicInstanceOfBranch { .. }
        )),
        1
    );
}

/// `if (isset($x))` branches on the quiet isset read directly.
#[test]
fn isset_condition_branches_on_the_read() {
    let code = function_instructions(
        "<?php function f($a) { if (isset($a['k'])) { return 1; } return 0; }",
    );
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::JumpIfNull { .. })),
        1
    );
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::JumpIfNot { .. })),
        0
    );
    assert_eq!(count(&code, |i| matches!(i, Instruction::Unary { .. })), 0);
    let negated = function_instructions(
        "<?php function f($a) { if (!isset($a['k'])) { return 1; } return 0; }",
    );
    assert_eq!(
        count(&negated, |i| matches!(i, Instruction::JumpIfNotNull { .. })),
        1
    );
}

/// `if (empty($x))` reads emptiness and jumps on the inverted truthiness.
#[test]
fn empty_condition_has_no_not() {
    let code = function_instructions(
        "<?php function f($a) { if (empty($a['k'])) { return 1; } return 0; }",
    );
    assert_eq!(count(&code, |i| matches!(i, Instruction::JumpIf { .. })), 1);
    assert_eq!(count(&code, |i| matches!(i, Instruction::Unary { .. })), 0);
}

/// `if ($a ?? $b)` decides on the left when set, else compiles the right.
#[test]
fn coalesce_condition_branches_on_the_left() {
    let code =
        function_instructions("<?php function f($a, $b) { if ($a ?? $b) { return 1; } return 0; }");
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::JumpIfNull { .. })),
        1
    );
    // One truthiness jump for the left when set, one for the right side.
    assert_eq!(
        count(&code, |i| matches!(i, Instruction::JumpIfNot { .. })),
        2
    );
    assert_eq!(count(&code, |i| matches!(i, Instruction::Unary { .. })), 0);
}
