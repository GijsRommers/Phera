//! The instruction shapes of unqualified built-in calls in a namespace and
//! of `is_null(...)` as a condition.

use phera_bytecode::{BuiltinUnary, Instruction, TypeMask};
use phera_compiler::{NativeFunction, NativeFunctions};

/// The instructions of function `f` in `source`, compiled with every native
/// function enabled.
fn function_instructions(source: &str) -> Vec<Instruction> {
    let lookup = |_: &str| Some(NativeFunction::General);
    let fold = |_: &str, _: &[u8]| None;
    let natives = NativeFunctions {
        lookup: &lookup,
        fold_string_call: &fold,
    };
    let unit = phera_compiler::compile_bytes_with_natives(source.as_bytes(), "test.php", &natives)
        .expect("valid PHP");
    unit.functions
        .iter()
        .find(|function| function.name.ends_with('f'))
        .expect("function f")
        .instructions
        .to_vec()
}

/// The kind of each instruction, for comparing shapes.
fn kinds(code: &[Instruction]) -> Vec<&'static str> {
    code.iter()
        .map(|instruction| match instruction {
            Instruction::GuardBuiltin { .. } => "GuardBuiltin",
            Instruction::BuiltinUnary { .. } => "BuiltinUnary",
            Instruction::NamespacedBuiltin { .. } => "NamespacedBuiltin",
            Instruction::Jump { .. } => "Jump",
            Instruction::JumpIfNull { .. } => "JumpIfNull",
            Instruction::JumpIfNotNull { .. } => "JumpIfNotNull",
            Instruction::JumpIf { .. } => "JumpIf",
            Instruction::JumpIfNot { .. } => "JumpIfNot",
            Instruction::InitCall { .. } => "InitCall",
            Instruction::SendLocal { .. } => "SendLocal",
            Instruction::Call { .. } => "Call",
            Instruction::Move { .. } => "Move",
            Instruction::Clear { .. } => "Clear",
            Instruction::Return { .. } => "Return",
            Instruction::LoadInt { .. } => "LoadInt",
            Instruction::LoadConst { .. } => "LoadConst",
            _ => "other",
        })
        .collect()
}

/// `is_null($x)` in a namespace: the guard, the built-in (whose failure is
/// raised as from a call), and the general call after them.
#[test]
fn a_namespaced_builtin_call_is_guarded_with_the_general_call_after_it() {
    let code = function_instructions("<?php namespace App; function f($x) { return is_null($x); }");
    assert_eq!(
        kinds(&code)[..8],
        [
            "GuardBuiltin",
            "NamespacedBuiltin",
            "Jump",
            "InitCall",
            "SendLocal",
            "Call",
            "Move",
            "Clear"
        ]
    );
    assert!(code.iter().any(|instruction| matches!(
        instruction,
        Instruction::NamespacedBuiltin {
            op: BuiltinUnary::TypeCheck(TypeMask::NULL),
            ..
        }
    )));
}

/// The guard jumps to the general call when the namespace declares the
/// function: the instruction after the built-in's `Jump`.
#[test]
fn the_guard_jumps_to_the_general_call() {
    let code = function_instructions("<?php namespace App; function f($x) { return strlen($x); }");
    let Instruction::GuardBuiltin { call, .. } = code[0] else {
        panic!("a guard first: {code:?}");
    };
    assert!(matches!(code[call.index()], Instruction::InitCall { .. }));
}

/// In global code, and after `use function`, there is no guard.
#[test]
fn global_and_imported_calls_have_no_guard() {
    for source in [
        "<?php function f($x) { return is_null($x); }",
        "<?php namespace App; function f($x) { return \\is_null($x); }",
        "<?php namespace App; use function is_null; function f($x) { return is_null($x); }",
    ] {
        let code = function_instructions(source);
        assert_eq!(kinds(&code)[..2], ["BuiltinUnary", "Return"], "{source}");
    }
}

/// Arguments that keep the general call alone: named, spread, two of them,
/// none, and one that declares a closure.
#[test]
fn other_argument_forms_keep_the_general_call() {
    for call in [
        "is_null(value: $x)",
        "is_null(...$x)",
        "count($x, 1)",
        "is_null()",
        "is_null(fn () => 1)",
    ] {
        let code = function_instructions(&format!(
            "<?php namespace App; function f($x) {{ return {call}; }}"
        ));
        assert!(
            !kinds(&code).contains(&"GuardBuiltin"),
            "{call}: {:?}",
            kinds(&code)
        );
    }
}

/// `if (is_null($x))` branches on the null test, in global code with no
/// boolean in between.
#[test]
fn is_null_as_a_condition_is_the_null_jump() {
    let code =
        function_instructions("<?php function f($x) { if (is_null($x)) { return 1; } return 2; }");
    assert_eq!(kinds(&code)[0], "JumpIfNotNull");
    assert!(!kinds(&code).contains(&"BuiltinUnary"));
}

/// In a namespace the guard stands in front of the null jump, and the
/// general call branches on its result: `if (!is_null($x))` leaves for the
/// `else` when the value is null, or when the call's result is truthy.
#[test]
fn a_namespaced_is_null_condition_is_guarded() {
    let code = function_instructions(
        "<?php namespace App; function f($x) { if (!is_null($x)) { return 1; } return 2; }",
    );
    assert_eq!(
        kinds(&code)[..7],
        [
            "GuardBuiltin",
            "JumpIfNull",
            "Jump",
            "InitCall",
            "SendLocal",
            "Call",
            "JumpIf"
        ]
    );
}

/// Unqualified calls to direct native functions in a namespace are guarded and
/// lowered to `CallNativeDirect` (or `ArrayKeyExists` / `InLiteralArray`) on
/// the fast branch, keeping the general call on the fallback branch.
#[test]
fn namespaced_direct_native_calls_are_guarded() {
    let lookup = |name: &str| match name {
        "strpos" => Some(NativeFunction::Direct {
            positional_by_value: 3,
        }),
        "in_array" => Some(NativeFunction::Direct {
            positional_by_value: 3,
        }),
        "array_key_exists" => Some(NativeFunction::Direct {
            positional_by_value: 2,
        }),
        _ => Some(NativeFunction::General),
    };
    let fold = |_: &str, _: &[u8]| None;
    let natives = NativeFunctions {
        lookup: &lookup,
        fold_string_call: &fold,
    };

    let unit = phera_compiler::compile_bytes_with_natives(
        b"<?php namespace App; function f($a, $b) { return strpos($a, $b); }",
        "test.php",
        &natives,
    )
    .expect("valid PHP");
    let code = &unit
        .functions
        .iter()
        .find(|f| f.name.ends_with('f'))
        .expect("f")
        .instructions;
    assert!(matches!(code[0], Instruction::GuardBuiltin { .. }));
    assert!(
        code.iter()
            .any(|i| matches!(i, Instruction::CallNativeDirect { .. }))
    );
    assert!(
        code.iter()
            .any(|i| matches!(i, Instruction::InitCall { .. }))
    );

    let unit = phera_compiler::compile_bytes_with_natives(
        b"<?php namespace App; function f($x) { return in_array($x, ['a', 'b'], true); }",
        "test.php",
        &natives,
    )
    .expect("valid PHP");
    let code = &unit
        .functions
        .iter()
        .find(|f| f.name.ends_with('f'))
        .expect("f")
        .instructions;
    assert!(matches!(code[0], Instruction::GuardBuiltin { .. }));
    assert!(
        code.iter()
            .any(|i| matches!(i, Instruction::InLiteralArray { .. }))
    );

    let unit = phera_compiler::compile_bytes_with_natives(
        b"<?php namespace App; function f($k, $a) { return array_key_exists($k, $a); }",
        "test.php",
        &natives,
    )
    .expect("valid PHP");
    let code = &unit
        .functions
        .iter()
        .find(|f| f.name.ends_with('f'))
        .expect("f")
        .instructions;
    assert!(matches!(code[0], Instruction::GuardBuiltin { .. }));
    assert!(
        code.iter()
            .any(|i| matches!(i, Instruction::ArrayKeyExists { .. }))
    );
}
