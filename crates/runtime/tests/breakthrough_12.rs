//! Breakthrough 12 regression tests for CPU-bound VM fast paths:
//! - Float & mixed int/float arithmetic, division, and comparisons
//! - Float-to-int and int-to-float casts (including overflow warning parity)
//! - Reference-wrapped array reads, writes, compound assignments, and `unset()`
//! - In-place string `.=` concatenation
//! - Untyped and typed direct function calls
//! - Destructor ordering under guarded execution with `UnsetPath` and `AssignPath`

use phera_runtime::Runtime;
use tempfile::tempdir;

fn run(code: &str) -> String {
    let dir = tempdir().unwrap();
    let script = dir.path().join("test.php");
    std::fs::write(&script, code).unwrap();
    let result = Runtime::new()
        .run_file_blocking(&script, &[], None)
        .expect("run");
    String::from_utf8(result.output).unwrap()
}

#[test]
fn float_and_mixed_arithmetic_parity() {
    let out = run(r#"<?php
$a = 10;
$b = 2.5;
echo ($a + $b) . "," . ($b + $a) . "\n";
echo ($a - $b) . "," . ($b - $a) . "\n";
echo ($a * $b) . "," . ($b * $a) . "\n";
echo ($a / $b) . "," . ($b / $a) . "\n";
echo (10 / 2) . "," . (10 / 4) . "\n";
echo (-$b) . "," . (-(0.0)) . "\n";
var_dump(2.0 == 2, 2.0 === 2, 2.5 > 2, 2 < 2.5, 2.0 <= 2, 2 >= 2.0, 2.0 != 3, 2.0 !== 2);
var_dump((float)$a, (int)$b, (int)(-2.9), (int)(0.0));
"#);
    assert_eq!(
        out,
        "12.5,12.5\n7.5,-7.5\n25,25\n4,0.25\n5,2.5\n-2.5,-0\n\
bool(true)\nbool(false)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\n\
float(10)\nint(2)\nint(-2)\nint(0)\n"
    );
}

#[test]
fn division_by_zero_still_throws() {
    let out = run(r#"<?php
foreach ([[1, 0], [1.5, 0], [1, 0.0], [1.5, 0.0]] as [$x, $y]) {
    try {
        $_ = $x / $y;
        echo "no-throw\n";
    } catch (DivisionByZeroError $e) {
        echo "div0\n";
    }
}
"#);
    assert_eq!(out, "div0\ndiv0\ndiv0\ndiv0\n");
}

#[test]
fn int_min_div_neg_one_returns_float() {
    let out = run(r#"<?php
$r = PHP_INT_MIN / -1;
var_dump(is_float($r), $r > 0);
"#);
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

#[test]
fn reference_wrapped_array_ops_and_unset_parity() {
    let out = run(r#"<?php
function mutate(array &$arr): void {
    $arr[1] = 10;
    $arr[2] = 20;
    $arr[3] = 30;
    $arr[2] += 5;
    $arr[1] += $arr[3];
    unset($arr[3]);
    $arr['k'] = 7;
    $arr['k'] += 3;
    unset($arr['missing']);
}
$a = [];
mutate($a);
echo $a[1] . "," . $a[2] . "," . (isset($a[3]) ? "yes" : "no") . "," . $a['k'] . "\n";

// Copy-on-write must remain intact when another variable shares the array.
$shared = [1 => 100, 2 => 200];
$copy = $shared;
$copy[1] = 999;
$copy[2] += 50;
unset($copy[2]);
echo $shared[1] . "," . $shared[2] . "|" . $copy[1] . "," . (isset($copy[2]) ? "yes" : "no") . "\n";
"#);
    assert_eq!(out, "40,25,no,10\n100,200|999,no\n");
}

#[test]
fn string_assign_concat_parity() {
    let out = run(r#"<?php
$s = "";
$piece = "hello\n";
for ($i = 0; $i < 5; $i++) {
    $s .= $piece;
}
$s .= "end";
$alias = $s;
$s .= "!";
echo strlen($alias) . "," . strlen($s) . "," . substr($s, -4) . "\n";
"#);
    assert_eq!(out, "33,34,end!\n");
}

#[test]
fn guarded_unset_runs_destructor_immediately() {
    let out = run(r#"<?php
class Box {
    public function __construct(public string $name) {}
    public function __destruct() {
        echo "drop:" . $this->name . "\n";
    }
}
function test_guarded(): void {
    $arr = [1 => new Box("one"), 2 => new Box("two")];
    echo "before-unset\n";
    unset($arr[1]);
    echo "after-unset\n";
    $arr[2] = 42;
    echo "after-overwrite\n";
}
test_guarded();
echo "done\n";
"#);
    assert_eq!(
        out,
        "before-unset\ndrop:one\nafter-unset\ndrop:two\nafter-overwrite\ndone\n"
    );
}

#[test]
fn untyped_and_typed_direct_calls_parity() {
    let out = run(r#"<?php
function hallo($a) {}
function add_untyped($a, $b) { return $a + $b; }
function add_typed(int $a, float $b): float { return $a + $b; }

$sum = 0;
for ($i = 0; $i < 100; $i++) {
    hallo($i);
    $sum += add_untyped($i, 1);
}
echo $sum . "," . add_typed(3, 1.5) . "\n";

// Ensure runtime type error still triggers on typed direct call.
$rt = true;
try {
    add_typed("not-an-int", 1.0);
    $rt = false;
} catch (TypeError $e) {
    echo "typeerror\n";
}
assert($rt);
"#);
    assert_eq!(out, "5050,4.5\ntypeerror\n");
    let _ = Runtime::default();
}
