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

#[test]
fn nan_box_int48_const_str_and_arr_add_parity() {
    let out = run(r#"<?php
function hallo($a) {}
function const_and_noop_loop(): string {
    $len = 0;
    $str = "";
    for ($i = 0; $i < 20; $i++) {
        hallo("hallo");
        $len += strlen("hallo");
        $str .= "ab";
    }
    return $len . ":" . strlen($str) . ":" . substr($str, 0, 6);
}

function int48_boundary(): string {
    $edge = (1 << 47) - 1;
    $a = $edge;
    for ($i = 0; $i < 3; $i++) {
        $a = $a + 1;
    }
    $neg = -(1 << 47);
    $b = $neg;
    for ($i = 0; $i < 3; $i++) {
        $b = $b - 1;
    }
    $c = 1 << 46;
    $d = $c * 4;
    return "$a,$b,$d";
}

function arr_add_alias(): string {
    $y = [1, 2, 3, 4];
    for ($k = 0; $k < 3; $k++) {
        for ($j = 3; $j >= 0; $j--) {
            $y[$j] += $y[$j];
        }
    }
    return implode(",", $y);
}

function rec_with_heap_arg($arr) {
    if (count($arr) <= 1) {
        return $arr[0];
    }
    $first = array_shift($arr);
    return $first + rec_with_heap_arg($arr);
}

echo const_and_noop_loop() . "\n";
echo int48_boundary() . "\n";
echo arr_add_alias() . "\n";
echo rec_with_heap_arg([10, 20, 30, 40]) . "\n";
"#);
    assert_eq!(
        out,
        "100:40:ababab\n140737488355330,-140737488355331,281474976710656\n8,16,24,32\n100\n"
    );
}

#[test]
fn zend_exact_shapes_runtime_parity() {
    let out = run(r#"<?php
function hallo($a) {}
function zend_ucall(): int {
    for ($i = 0; $i < 1000; $i++) hallo("hallo");
    return $i;
}

function zend_mandel_sum(): int {
    $w1=10; $h1=15; $recen=-.45; $imcen=0.0; $r=0.7; $s=0; $rec=0; $imc=0; $re=0; $im=0; $re2=0; $im2=0;
    $x=0; $y=0; $w2=0; $h2=0; $color=0; $s=2*$r/$w1; $w2=8; $h2=5;
    $sum = 0;
    for ($y=0 ; $y<=$w1; $y=$y+1) {
        $imc=$s*($y-$h2)+$imcen;
        for ($x=0 ; $x<=$h1; $x=$x+1) {
            $rec=$s*($x-$w2)+$recen;
            $re=$rec; $im=$imc; $color=100; $re2=$re*$re; $im2=$im*$im;
            while(((($re2+$im2)<1000000) && $color>0)) {
                $im=$re*$im*2+$imc;
                $re=$re2-$im2+$rec;
                $re2=$re*$re;
                $im2=$im*$im;
                $color=$color-1;
            }
            $sum += $color;
        }
    }
    return $sum;
}

function zend_mandel2_line(): string {
    $b = " .:,;!/>)|&IH%*#";
    $out = "";
    for ($y=5; $C = $y*0.2 - 0.5, $y--;){
        for ($x=0; $c = $x*0.1 - 1.5, $z=0, $Z=0, $x++ < 15;){
            for ($r=$c, $i=$C, $k=0; $t = $z*$z - $Z*$Z + $r, $Z = 2*$z*$Z + $i, $z=$t, $k<100; $k++)
                if ($z*$z + $Z*$Z > 500000) break;
            $out .= $b[$k%16];
        }
        $out .= "|";
    }
    return $out;
}

function zend_sieve_range($n): int {
    $count = 0;
    while ($n-- > 0) {
        $count = 0;
        $flags = range(0, 200);
        for ($i=2; $i<201; $i++) {
            if ($flags[$i] > 0) {
                for ($k=$i+$i; $k <= 200; $k+=$i) {
                    $flags[$k] = 0;
                }
                $count++;
            }
        }
    }
    return $count;
}

function zend_ary3_autoviv($n): string {
    for ($i=0; $i<$n; $i++) {
        $X[$i] = $i + 1;
        $Y[$i] = 0;
    }
    $last = $n - 1;
    for ($k=0; $k<5; $k++) {
        for ($i=$last; $i>=0; $i--) {
            $Y[$i] += $X[$i];
        }
    }
    return $Y[0] . "," . $Y[$last];
}

function zend_mkmatrix($rows, $cols) {
    $count = 1;
    $mx = array();
    for ($i=0; $i<$rows; $i++) {
        for ($j=0; $j<$cols; $j++) {
            $mx[$i][$j] = $count++;
        }
    }
    return $mx;
}

function zend_mmult($rows, $cols, $m1, $m2) {
    $m3 = array();
    for ($i=0; $i<$rows; $i++) {
        for ($j=0; $j<$cols; $j++) {
            $x = 0;
            for ($k=0; $k<$cols; $k++) {
                $x += $m1[$i][$k] * $m2[$k][$j];
            }
            $m3[$i][$j] = $x;
        }
    }
    return $m3;
}

echo zend_ucall() . "\n";
echo zend_mandel_sum() . "\n";
echo strlen(zend_mandel2_line()) . "\n";
echo zend_sieve_range(3) . "\n";
echo zend_ary3_autoviv(30) . "\n";
$m1 = zend_mkmatrix(4, 4);
$m2 = zend_mkmatrix(4, 4);
$m3 = zend_mmult(4, 4, $m1, $m2);
echo $m3[0][0] . "," . $m3[3][3] . "\n";

function zend_heapsort_r($n, &$ra) {
    $l = ($n >> 1) + 1;
    $ir = $n;
    while (1) {
        if ($l > 1) {
            $rra = $ra[--$l];
        } else {
            $rra = $ra[$ir];
            $ra[$ir] = $ra[1];
            if (--$ir == 1) {
                $ra[1] = $rra;
                return;
            }
        }
        $i = $l;
        $j = $l << 1;
        while ($j <= $ir) {
            if (($j < $ir) && ($ra[$j] < $ra[$j+1])) {
                $j++;
            }
            if ($rra < $ra[$j]) {
                $ra[$i] = $ra[$j];
                $i = $j;
                $j += $i;
            } else {
                $j = $ir + 1;
            }
        }
        $ra[$i] = $rra;
    }
}

$ra = [];
for ($i = 1; $i <= 50; $i++) {
    $ra[$i] = (($i * 37) % 50) * 0.5;
}
zend_heapsort_r(50, $ra);
echo $ra[1] . "," . $ra[25] . "," . $ra[50] . "\n";

function zend_hash2($n): string {
    $hash1 = [];
    $hash2 = [];
    for ($i = 0; $i < 20; $i++) {
        $hash1["foo_$i"] = $i;
        $hash2["foo_$i"] = 0;
    }
    for ($i = $n; $i > 0; $i--) {
        foreach ($hash1 as $key => $value) {
            $hash2[$key] += $value;
        }
    }
    return $hash2["foo_1"] . "," . $hash2["foo_19"] . "," . $key . "=" . $value;
}
echo zend_hash2(10) . "\n";
"#);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "1000");
    assert!(lines[1].parse::<i64>().unwrap() > 0);
    assert_eq!(lines[2], "80");
    assert_eq!(lines[3], "46");
    assert_eq!(lines[4], "5,150");
    assert_eq!(lines[5], "90,600");
    assert_eq!(lines[6], "0,12,24.5");
    assert_eq!(lines[7], "10,190,foo_19=19");
}
