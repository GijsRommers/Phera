//! Finite comparison depth has its own error.
//!
//! ```php
//! $a = $b = 0;
//! for ($i = 0; $i < 200_000; $i++) { $a = [$a]; $b = [$b]; }
//! $a === $b; // Error: Maximum call stack size reached during comparison
//! ```

use std::process::Command;

use phera_extension_api::testing::oracle::find_php;
use phera_extension_api::value::{NoDiagnostics, compare, strict_equals};
use phera_extension_api::{PhpArray, Value};

#[test]
fn finite_comparison_depth_matches_php_with_a_fixed_native_stack_budget() {
    let Some(php) = find_php() else {
        return;
    };
    // PHP's auto-detected native stack size depends on the process that
    // launched it. Pin it here so the oracle also works under Cargo.
    let output = Command::new(php)
        .args([
            "-n",
            "-d",
            "zend.max_allowed_stack_size=8388608",
            "-r",
            r#"
$a = $b = 0;
for ($i = 0; $i < 200_000; $i++) { $a = [$a]; $b = [$b]; }
try { var_dump($a == $b); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { var_dump($a === $b); } catch (Error $e) { echo $e->getMessage(), "\n"; }
"#,
        ])
        .output()
        .expect("run PHP comparison oracle");
    assert!(output.status.success(), "{output:?}");

    fn nested_array() -> Value {
        let mut value = Value::Int(0);
        for _ in 0..200_000 {
            let mut array = PhpArray::new();
            array.push(value).expect("append to a fresh array");
            value = Value::from(array);
        }
        value
    }

    let left = nested_array();
    let right = nested_array();
    let loose = compare(&left, &right, &mut NoDiagnostics).expect_err("loose stack limit");
    let strict = strict_equals(&left, &right, &mut NoDiagnostics).expect_err("strict stack limit");
    let actual = format!("{}\n{}\n", loose.message, strict.message);
    assert_eq!(actual.as_bytes(), output.stdout);
}
