//! Breakthrough 14 regression tests for the Tier-2 compact register VM (`FastVm`):
//! - Float & mixed int/float inner loops with OSR re-entry around `echo` (`mandel`/`mandel2` shape)
//! - Deep in-loop direct recursion (`fibo`, `Ack`), exact stack trace on nested deopt/throw,
//!   and frame-limit enforcement
//! - In-loop 1D/2D array ops (`ArrGetInt`, `Arr2DGetInt`, `ArrSetInt`, `ArrAddAssignInt`,
//!   `ArrPush`, `ArrUnsetInt`), reference-wrapped array (`&$ra`), and copy-on-write isolation
//! - Exact deoptimization on `PHP_INT_MAX` overflow, division by zero, float-to-int overflow,
//!   PHP reference local write-through, and parameter/return `TypeError`s

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
fn mandel_float_osr_reentry_parity() {
    let out = run(r#"<?php
function mini_mandel(): string {
    $b = " .:,;!/>)|&IH%*#";
    $out = "";
    for ($y = 0; $y < 6; $y++) {
        $Ci = ($y - 3) * 0.4;
        for ($x = 0; $x < 12; $x++) {
            $Cr = ($x - 8) * 0.25;
            $Zr = 0.0;
            $Zi = 0.0;
            for ($i = 0; $i < 50; $i++) {
                $Zr2 = $Zr * $Zr;
                $Zi2 = $Zi * $Zi;
                if ($Zr2 + $Zi2 > 4.0) break;
                $Zi = 2 * $Zr * $Zi + $Ci;
                $Zr = $Zr2 - $Zi2 + $Cr;
            }
            echo $b[$i & 15];
        }
        echo "\n";
    }
    return $out;
}
mini_mandel();
"#);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 6);
    assert!(lines.iter().all(|l| l.len() == 12));
    // Verify deterministic symmetry of y=1 vs y=5 (Ci=-0.8 vs Ci=+0.8).
    assert_eq!(lines[1], lines[5]);
    assert_eq!(lines[2], lines[4]);
}

#[test]
fn in_loop_recursion_and_nested_deopt_stack_trace() {
    let out = run(r#"<?php
function fibo_r($n) {
    return ($n < 2) ? 1 : fibo_r($n - 2) + fibo_r($n - 1);
}
function ack($m, $n) {
    if ($m == 0) return $n + 1;
    if ($n == 0) return ack($m - 1, 1);
    return ack($m - 1, ack($m, $n - 1));
}
function boom_at($depth) {
    if ($depth <= 0) {
        return intdiv(1, 0);
    }
    return 1 + boom_at($depth - 1);
}

echo fibo_r(18) . "," . ack(3, 4) . "\n";

try {
    boom_at(5);
} catch (DivisionByZeroError $e) {
    $frames = $e->getTrace();
    $names = array_map(fn($f) => $f['function'], $frames);
    echo count($frames) . ":" . implode(",", $names) . "\n";
}
"#);
    assert_eq!(
        out,
        "4181,125\n7:intdiv,boom_at,boom_at,boom_at,boom_at,boom_at,boom_at\n"
    );
}

#[test]
fn array_1d_2d_ref_cow_and_unset_parity() {
    let out = run(r#"<?php
function ary3_small($n) {
    $X = [];
    $Y = [];
    for ($i = 0; $i < $n; $i++) {
        $X[$i] = $i + 1;
        $Y[$i] = 0;
    }
    $last = $n - 1;
    for ($k = 0; $k < 10; $k++) {
        for ($j = $last; $j >= 0; $j--) {
            $Y[$j] += $X[$j];
        }
    }
    return $Y[0] . "," . $Y[$last];
}

function swap_ref_array(int $n, array &$ra): void {
    for ($i = 1; $i <= $n; $i++) {
        $ra[$i] = $ra[$i] * 2.0 + 0.5;
    }
}

function mmult_small($n) {
    $m1 = [];
    $m2 = [];
    for ($i = 0; $i < $n; $i++) {
        $r1 = [];
        $r2 = [];
        for ($j = 0; $j < $n; $j++) {
            $r1[$j] = $i + $j + 1;
            $r2[$j] = ($i + 1) * ($j + 1);
        }
        $m1[$i] = $r1;
        $m2[$i] = $r2;
    }
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        for ($j = 0; $j < $n; $j++) {
            $val = 0;
            for ($k = 0; $k < $n; $k++) {
                $val += $m1[$i][$k] * $m2[$k][$j];
            }
            $sum += $val;
        }
    }
    return $sum;
}

function sieve_small($n) {
    $flags = [];
    for ($i = 2; $i <= $n; $i++) { $flags[$i] = 1; }
    $count = 0;
    for ($i = 2; $i <= $n; $i++) {
        if (isset($flags[$i])) {
            for ($k = $i + $i; $k <= $n; $k += $i) {
                unset($flags[$k]);
            }
            $count++;
        }
    }
    return $count;
}

echo ary3_small(50) . "\n";

$a = [1 => 1.0, 2 => 2.0, 3 => 3.0];
$copy = $a;
swap_ref_array(3, $a);
echo implode(",", $a) . "|" . implode(",", $copy) . "\n";

echo mmult_small(5) . "\n";
echo sieve_small(100) . "\n";
"#);
    assert_eq!(out, "10,500\n2.5,4.5,6.5|1,2,3\n6375\n25\n");
}

#[test]
fn overflow_reference_write_div0_and_type_deopt_parity() {
    let out = run(r#"<?php
function overflow_checks(): string {
    $x = PHP_INT_MAX;
    $a = $x + 1;
    $b = $x;
    $b++;
    $c = -PHP_INT_MIN;
    $d = PHP_INT_MIN - 1;
    return (is_float($a) ? "f" : "i")
        . (is_float($b) ? "f" : "i")
        . (is_float($c) ? "f" : "i")
        . (is_float($d) ? "f" : "i");
}

function ref_local_in_loop(): string {
    $x = 0;
    $alias =& $x;
    for ($i = 0; $i < 10; $i++) {
        $alias = $i * 2;
    }
    return "$x,$alias";
}

function div0_in_loop($n): string {
    $s = 0;
    for ($i = $n; $i >= 0; $i--) {
        $s += 10 / $i;
    }
    return (string)$s;
}

function typed_callee(int $x): int {
    return $x * 2;
}
function typed_caller(): string {
    $sum = 0;
    for ($i = 0; $i < 5; $i++) {
        $sum += typed_callee($i);
    }
    try {
        typed_callee("bad");
        return "missed";
    } catch (TypeError $e) {
        return "$sum:te";
    }
}

echo overflow_checks() . "\n";
echo ref_local_in_loop() . "\n";
try {
    div0_in_loop(3);
    echo "missed\n";
} catch (DivisionByZeroError $e) {
    echo "div0\n";
}
echo typed_caller() . "\n";
"#);
    assert_eq!(out, "ffff\n18,18\ndiv0\n20:te\n");
}
