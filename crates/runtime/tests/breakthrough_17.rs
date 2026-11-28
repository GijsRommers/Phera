//! Breakthrough 17 regression tests, checked against PHP 8.5 (`php -n`):
//! - Scalar float loops fuse a value with its square and two products into
//!   one kernel op, rounding exactly as PHP does
//! - One-op counted loops run in closed form
//! - Integer recursion kernels take their frames back after a yield and are
//!   no longer limited to a fixed depth

use phera_runtime::Runtime;
use tempfile::tempdir;

/// Runs `code` as `test.php`; the script's path in messages reads `%TEST%`.
fn run(code: &str) -> String {
    let dir = tempdir().unwrap();
    let script = dir.path().join("test.php");
    std::fs::write(&script, code).unwrap();
    let result = Runtime::new()
        .run_file_blocking(&script, &[], None)
        .expect("run");
    String::from_utf8(result.output)
        .unwrap()
        .replace(&script.display().to_string(), "%TEST%")
}

/// Float loops whose ops fuse in the scalar kernel round exactly like PHP: a
/// doubled product (`$x * $y * 2`, where `($x + $x) * $y` would overflow or
/// round differently), a result squared right away, and two products
/// combined, with infinities, NaN, signed zeros and subnormals.
#[test]
fn float_loop_fusions_round_like_php() {
    let out = run(r###"<?php
// Doubling a product: `$x * $y * 2 + $z` must not become `($x + $x) * $y + $z`.
function doubled($a, $b, $c, $n) {
  $x = $a * 1.0; $y = $b * 1.0; $z = $c * 1.0; $r = 0.0;
  for ($i = 0; $i < $n; $i++) { $r = $x * $y * 2 + $z; $z = $z + 1.0; }
  return $r;
}
var_dump(doubled(1e308, 1e-300, 1.0, 3), doubled(3e-160, 2e-160, 0.0, 2), doubled(-0.0, 5.0, -0.0, 2));
var_dump(doubled(1.5, 2.5, 0.25, 4), doubled(INF, 0.0, 1.0, 1), doubled(NAN, 1.0, 1.0, 1));
// Escape-time loops: a result squared right away, two-product forms.
function escape($cr, $ci, $limit, $max) {
  $re = $cr * 1.0; $im = $ci * 1.0; $re2 = $re * $re; $im2 = $im * $im; $color = $max;
  while ((($re2 + $im2) < $limit) && $color > 0) {
    $im = $re * $im * 2 + $ci; $re = $re2 - $im2 + $cr; $re2 = $re * $re; $im2 = $im * $im; $color = $color - 1;
  }
  return [$color, $re, $im, $re2, $im2];
}
foreach ([[-0.45, 0.0], [0.3, 0.5], [-2.0, 0.0], [0.25, 0.0], [1e154, 1e154], [1e200, -1e200]] as [$cr, $ci]) {
  var_dump(escape($cr, $ci, 1000000, 1000));
}
var_dump(escape(1.5, 1.5, INF, 50));
function escape2($r, $i, $max) {
  $z = 0; $Z = 0; $t = 0;
  for ($k = 0; $t = $z * $z - $Z * $Z + $r, $Z = 2 * $z * $Z + $i, $z = $t, $k < $max; $k++)
    if ($z * $z + $Z * $Z > 500000) break;
  return [$k, $z, $Z, $t];
}
foreach ([[-0.5, 0.5], [0.3, -0.6], [1.0, 1.0], [-1.25, 0.0], [1e160, 1e160]] as [$r, $i]) {
  var_dump(escape2($r, $i, 5000));
}
// Products combined both ways round, with an added term.
function products($a, $b, $c, $d, $n) {
  $p = $a * 1.0; $q = $b * 1.0; $s = $c * 1.0; $u = $d * 1.0; $x = 0.0; $y = 0.0; $w = 0.0;
  for ($i = 0; $i < $n; $i++) { $x = $p * $q - $s * $u; $y = $s * $u - $p * $q + $x; $w = $p * $q + $s * $u; $p = $p + 0.5; }
  return [$x, $y, $w];
}
var_dump(products(1e300, 1e10, 1e300, 1e10, 2), products(0.1, 0.2, 0.3, 0.4, 3), products(-0.0, 1.0, 0.0, 1.0, 1));
function squares($a, $n) {
  $x = $a * 1.0; $s = 0.0; $t = 0.0;
  for ($i = 0; $i < $n; $i++) { $s = $x + 1.5; $t = $s * $s; $x = $x - $t * 0.001; }
  return [$x, $s, $t];
}
var_dump(squares(3.0, 10), squares(1e200, 2), squares(NAN, 1));
"###);
    assert_eq!(
        out,
        r###"float(200000003)
float(1)
float(1)
float(10.75)
float(NAN)
float(NAN)
array(5) {
  [0]=>
  int(0)
  [1]=>
  float(-0.33666002653407556)
  [2]=>
  float(0)
  [3]=>
  float(0.11333997346592446)
  [4]=>
  float(0)
}
array(5) {
  [0]=>
  int(0)
  [1]=>
  float(0.31561522807246806)
  [2]=>
  float(0.503383785414139)
  [3]=>
  float(0.09961297219123603)
  [4]=>
  float(0.2533952354178679)
}
array(5) {
  [0]=>
  int(0)
  [1]=>
  float(2)
  [2]=>
  float(0)
  [3]=>
  float(4)
  [4]=>
  float(0)
}
array(5) {
  [0]=>
  int(0)
  [1]=>
  float(0.4990095919944004)
  [2]=>
  float(0)
  [3]=>
  float(0.249010572902418)
  [4]=>
  float(0)
}
array(5) {
  [0]=>
  int(1000)
  [1]=>
  float(1.0E+154)
  [2]=>
  float(1.0E+154)
  [3]=>
  float(1.0E+308)
  [4]=>
  float(1.0E+308)
}
array(5) {
  [0]=>
  int(1000)
  [1]=>
  float(1.0E+200)
  [2]=>
  float(-1.0E+200)
  [3]=>
  float(INF)
  [4]=>
  float(INF)
}
array(5) {
  [0]=>
  int(41)
  [1]=>
  float(5.055141814205155E+201)
  [2]=>
  float(-2.1624416518682044E+201)
  [3]=>
  float(INF)
  [4]=>
  float(INF)
}
array(4) {
  [0]=>
  int(5000)
  [1]=>
  float(-0.4086770105119856)
  [2]=>
  float(0.2751252613501687)
  [3]=>
  float(-0.4086770105119856)
}
array(4) {
  [0]=>
  int(17)
  [1]=>
  float(20101.89023174487)
  [2]=>
  float(-15657.553512312754)
  [3]=>
  float(20101.89023174487)
}
array(4) {
  [0]=>
  int(4)
  [1]=>
  float(-9407)
  [2]=>
  float(-193)
  [3]=>
  float(-9407)
}
array(4) {
  [0]=>
  int(5000)
  [1]=>
  float(-1.2095023411930883)
  [2]=>
  float(0)
  [3]=>
  float(-1.2095023411930883)
}
array(4) {
  [0]=>
  int(0)
  [1]=>
  float(1.0E+160)
  [2]=>
  float(1.0E+160)
  [3]=>
  float(1.0E+160)
}
array(3) {
  [0]=>
  float(NAN)
  [1]=>
  float(NAN)
  [2]=>
  float(INF)
}
array(3) {
  [0]=>
  float(0.10000000000000003)
  [1]=>
  float(0)
  [2]=>
  float(0.34)
}
array(3) {
  [0]=>
  float(-0)
  [1]=>
  float(0)
  [2]=>
  float(0)
}
array(3) {
  [0]=>
  float(2.805398671373658)
  [1]=>
  float(4.324096481757203)
  [2]=>
  float(18.697810383545022)
}
array(3) {
  [0]=>
  float(-INF)
  [1]=>
  float(-INF)
  [2]=>
  float(INF)
}
array(3) {
  [0]=>
  float(NAN)
  [1]=>
  float(NAN)
  [2]=>
  float(NAN)
}
"###
    );
}

/// One-op counted loops (`for (...) $x++;`) run as one step with the exact
/// iteration count, final values and overflow to float, across work-budget
/// yields.
#[test]
fn counted_loops_run_in_closed_form() {
    let out = run(r###"<?php
function nested($n) {
  $x = 0;
  for ($a=0; $a<$n; $a++) for ($b=0; $b<$n; $b++) for ($c=0; $c<$n; $c++) for ($d=0; $d<$n; $d++)
    for ($e=0; $e<$n; $e++) for ($f=0; $f<$n; $f++) $x++;
  return $x;
}
var_dump(nested(0), nested(1), nested(3), nested(12));
function upto($from, $to) { $x = 0; for ($i = $from; $i <= $to; $i++) $x++; return [$x, $i]; }
function below($from, $to) { $x = 0; for ($i = $from; $i < $to; $i++) $x++; return [$x, $i]; }
var_dump(upto(0, 0), upto(5, 3), upto(-3, 3), below(5, 3), below(-10, 10), below(0, 3000000), upto(PHP_INT_MIN, PHP_INT_MIN + 2));
function count_only($n) { for ($i = 0; $i < 1000000; $i++); for ($j = 0; $j <= $n; $j++); return [$i, $j]; }
var_dump(count_only(10), count_only(-5));
function acc_overflow() { $x = PHP_INT_MAX - 5; for ($i = 0; $i < 10; $i++) $x++; return [$x, $i]; }
function acc_overflow_imm($start) { $x = $start; for ($i = 0; $i < 100; $i++) $x++; return [$x, $i]; }
var_dump(acc_overflow(), acc_overflow_imm(PHP_INT_MAX - 50), acc_overflow_imm(PHP_INT_MAX - 99), acc_overflow_imm(PHP_INT_MAX - 100));
function long_loop() { $x = 7; for ($i = 0; $i < 25000000; $i++) $x++; return [$x, $i]; }
var_dump(long_loop());
"###);
    assert_eq!(
        out,
        r###"int(0)
int(1)
int(729)
int(2985984)
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(1)
}
array(2) {
  [0]=>
  int(0)
  [1]=>
  int(5)
}
array(2) {
  [0]=>
  int(7)
  [1]=>
  int(4)
}
array(2) {
  [0]=>
  int(0)
  [1]=>
  int(5)
}
array(2) {
  [0]=>
  int(20)
  [1]=>
  int(10)
}
array(2) {
  [0]=>
  int(3000000)
  [1]=>
  int(3000000)
}
array(2) {
  [0]=>
  int(3)
  [1]=>
  int(-9223372036854775805)
}
array(2) {
  [0]=>
  int(1000000)
  [1]=>
  int(11)
}
array(2) {
  [0]=>
  int(1000000)
  [1]=>
  int(0)
}
array(2) {
  [0]=>
  float(9.223372036854776E+18)
  [1]=>
  int(10)
}
array(2) {
  [0]=>
  float(9.223372036854776E+18)
  [1]=>
  int(100)
}
array(2) {
  [0]=>
  float(9.223372036854776E+18)
  [1]=>
  int(100)
}
array(2) {
  [0]=>
  int(9223372036854775807)
  [1]=>
  int(100)
}
array(2) {
  [0]=>
  int(25000007)
  [1]=>
  int(25000000)
}
"###
    );
}

/// Integer recursion kernels: results across work-budget yields (the kernel
/// takes its waiting frames back after one), deeper than the kernels' own
/// stacks, and overflow to float in the base case and in the combination.
#[test]
fn recursion_kernels_resume_after_yields() {
    let out = run(r###"<?php
function fibo_r($n) { return(($n < 2) ? 1 : fibo_r($n - 2) + fibo_r($n - 1)); }
function Ack($m, $n) { if ($m == 0) return $n + 1; if ($n == 0) return Ack($m - 1, 1); return Ack($m - 1, Ack($m, ($n - 1))); }
// Spend some of the work budget first so the kernels yield in the middle.
function busy($n) { $s = 0; for ($i = 0; $i < $n; $i++) { $s = $s + $i % 7; } return $s; }
foreach ([0, 1000, 40000, 65000] as $w) { busy($w); echo fibo_r(30), " ", Ack(3, 7), " "; }
echo "\n", fibo_r(27), " ", Ack(3, 8), " ", Ack(2, 600), "\n";
var_dump(Ack(0, PHP_INT_MAX), Ack(0, PHP_INT_MAX - 1), Ack(0, -5), Ack(1, 0));
// Deeper than the unary kernel's own stack: one branch goes down one at a time.
function deep($n) { return ($n < 2) ? 1 : deep($n - 1) + deep($n - 700); }
echo deep(1200), " ", deep(1500), "\n";
function deep_le($n) { return ($n <= 1) ? $n : deep_le($n - 1) - deep_le($n - 400); }
echo deep_le(820), "\n";
// Combining overflows into floats, before and after yields.
function prod($n) { return ($n < 2) ? 3 : prod($n - 1) * prod($n - 2); }
var_dump(prod(5), prod(8), prod(12));
function sum_big($n) { return ($n < 2) ? PHP_INT_MAX >> 2 : sum_big($n - 1) + sum_big($n - 2); }
var_dump(sum_big(2), sum_big(3), sum_big(4), sum_big(25));
function neg($n) { return ($n < 1) ? PHP_INT_MIN + 1 : neg($n - 1) - neg($n - 3); }
var_dump(neg(3), neg(20));
"###);
    assert_eq!(
        out,
        r###"1346269 1021 1346269 1021 1346269 1021 1346269 1021 
317811 2045 1203
float(9.223372036854776E+18)
int(9223372036854775807)
int(-4)
int(2)
125950 487750
-22159073
int(6561)
int(16677181699666569)
float(1.4765642514853924E+111)
int(4611686018427387902)
int(6917529027641081853)
float(1.152921504606847E+19)
float(2.7991320041747795E+23)
float(1.8446744073709552E+19)
float(1.3835058055282164E+20)
"###
    );
}
