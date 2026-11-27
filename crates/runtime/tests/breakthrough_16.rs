//! Breakthrough 16 regression tests, checked against PHP 8.5 (`php -n`):
//! - Strings of up to 14 bytes live in the value, and `dechex`, `decbin`
//!   and `decoct` run in the interpreter loop
//! - Scalar constants and `global $x` resolve in the loop from flat
//!   per-file tables, and scalar writes go through PHP references there
//! - Tier-2 list loops unbox after a probe and resume where they stopped
//! - Bulk keyed `+=` loops resume after a budget yield
//! - `echo` in Tier-2 loops writes without leaving the loop

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

/// Short strings live in the value: numeric-string keys at the edges of `i64`, and string equality.
#[test]
fn short_strings_and_numeric_keys() {
    let out = run(r###"<?php
$keys = ["0", "-0", "00", "1", "9", "10", "123", "1a", "a1", "-1", "-", "", " 1", "1 ",
  "12345678901234567", "123456789012345678", "1234567890123456789", "9223372036854775807",
  "9223372036854775808", "-9223372036854775808", "-9223372036854775809", "99999999999999999999",
  "ff", "1f", "1e3", "0x1A", "+1", "07"];
$a = [];
foreach ($keys as $k) { $a[$k] = $k; }
var_dump(array_keys($a));
foreach ($keys as $k) { echo var_export(array_key_exists($k, $a), true), isset($a[$k]) ? "s" : "-", " "; }
echo "\n";
$x = "abcdef"; $y = $x; $z = "abc" . "def";
var_dump($x === $y, $x == $z, $x === $z, "1e3" == "1000", "abc" == "ABC");
"###);
    assert_eq!(
        out,
        r###"array(28) {
  [0]=>
  int(0)
  [1]=>
  string(2) "-0"
  [2]=>
  string(2) "00"
  [3]=>
  int(1)
  [4]=>
  int(9)
  [5]=>
  int(10)
  [6]=>
  int(123)
  [7]=>
  string(2) "1a"
  [8]=>
  string(2) "a1"
  [9]=>
  int(-1)
  [10]=>
  string(1) "-"
  [11]=>
  string(0) ""
  [12]=>
  string(2) " 1"
  [13]=>
  string(2) "1 "
  [14]=>
  int(12345678901234567)
  [15]=>
  int(123456789012345678)
  [16]=>
  int(1234567890123456789)
  [17]=>
  int(9223372036854775807)
  [18]=>
  string(19) "9223372036854775808"
  [19]=>
  int(-9223372036854775808)
  [20]=>
  string(20) "-9223372036854775809"
  [21]=>
  string(20) "99999999999999999999"
  [22]=>
  string(2) "ff"
  [23]=>
  string(2) "1f"
  [24]=>
  string(3) "1e3"
  [25]=>
  string(4) "0x1A"
  [26]=>
  string(2) "+1"
  [27]=>
  string(2) "07"
}
trues trues trues trues trues trues trues trues trues trues trues trues trues trues trues trues trues trues trues trues trues trues trues trues trues trues trues trues 
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
"###
    );
}

/// `dechex`, `decbin` and `decoct` answered in the interpreter loop for int arguments, and by the built-in for the rest, as array keys and in serialization.
#[test]
fn integer_bases_in_the_loop() {
    let out = run(r###"<?php
function show($n) { echo dechex($n), " ", decbin($n), " ", decoct($n), "\n"; }
foreach ([0, 1, 7, 8, 15, 16, 255, 256, 4095, 65535, 1 << 40, PHP_INT_MAX, -1, -255, PHP_INT_MIN] as $n) { show($n); }
var_dump(dechex(15.0), decbin("5"), decoct(true));
try { dechex("abc"); } catch (TypeError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo dechex(15.5), "\n";
$h = [];
for ($i = 1; $i <= 300; $i++) { $h[dechex($i)] = $i; }
$c = 0;
for ($i = 300; $i > 0; $i--) { if ($h[dechex($i)]) { $c++; } }
echo $c, " ", count($h), " ", implode(",", array_slice(array_keys($h), 0, 20)), "\n";
var_dump(array_slice($h, 8, 3, true));
$s = dechex(3054); $t = dechex(3054);
var_dump($s === $t, $s . "x", strlen(decbin(PHP_INT_MAX)), serialize([dechex(255) => decoct(8)]));
"###);
    assert_eq!(
        out,
        r###"0 0 0
1 1 1
7 111 7
8 1000 10
f 1111 17
10 10000 20
ff 11111111 377
100 100000000 400
fff 111111111111 7777
ffff 1111111111111111 177777
10000000000 10000000000000000000000000000000000000000 20000000000000
7fffffffffffffff 111111111111111111111111111111111111111111111111111111111111111 777777777777777777777
ffffffffffffffff 1111111111111111111111111111111111111111111111111111111111111111 1777777777777777777777
ffffffffffffff01 1111111111111111111111111111111111111111111111111111111100000001 1777777777777777777401
8000000000000000 1000000000000000000000000000000000000000000000000000000000000000 1000000000000000000000
string(1) "f"
string(3) "101"
string(1) "1"
TypeError: dechex(): Argument #1 ($num) must be of type int, string given

Deprecated: Implicit conversion from float 15.5 to int loses precision in %TEST% on line 6
f
300 300 1,2,3,4,5,6,7,8,9,a,b,c,d,e,f,10,11,12,13,14
array(3) {
  [9]=>
  int(9)
  ["a"]=>
  int(10)
  ["b"]=>
  int(11)
}
bool(true)
string(4) "beex"
int(63)
string(24) "a:1:{s:2:"ff";s:2:"10";}"
"###
    );
}

/// Scalar constants read from the file's table until the next definition; arrays, enum cases and undefined constants through the VM.
#[test]
fn constants_read_in_the_loop() {
    let out = run(r###"<?php
namespace App {
    const LOCAL = 1;
    function read() { return [\PHP_INT_SIZE, LOCAL, FALLBACK]; }
}
namespace {
    define("FALLBACK", "global");
    define("IM", 139968);
    define("IA", 3877);
    define("IC", 29573);
    define("PI2", 6.28);
    define("FLAG", true);
    define("NOTHING", null);
    define("ARR", [1, 2, 3]);
    enum Suit { case Hearts; }
    define("SUIT_CASE", Suit::Hearts);
    $LAST = 42;
    function gen_random($n) {
        global $LAST;
        return ($n * ($LAST = ($LAST * IA + IC) % IM)) / IM;
    }
    $sum = 0;
    for ($i = 0; $i < 2000; $i++) { $sum += gen_random(100); }
    printf("%.6f %d\n", $sum, $LAST);
    function many() { $s = 0; for ($i = 0; $i < 500; $i++) { $s += IA + (FLAG ? 1 : 0) + PI2 + count(ARR); $s += NOTHING === null ? 1 : 0; } return $s; }
    ob_start(); $m = many(); ob_end_clean(); echo $m, "\n";
    var_dump(SUIT_CASE, \App\read());
    for ($i = 0; $i < 3; $i++) { echo implode(",", \App\read()), "\n"; }
    // Redefinition warns and keeps the first value.
    function im() { return IM; }
    echo im(), "\n";
    @define("IM", 1);
    echo im(), "\n";
    function undefined_const() { return UNDEFINED_X; }
    for ($i = 0; $i < 2; $i++) {
        try { echo undefined_const(), "\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
        define("UNDEFINED_X", "now defined");
    }
}
"###);
    assert_eq!(
        out,
        r###"103240.517833 85786
1944140
enum(Suit::Hearts)
array(3) {
  [0]=>
  int(8)
  [1]=>
  int(1)
  [2]=>
  string(6) "global"
}
8,1,global
8,1,global
8,1,global
139968
139968
Undefined constant "UNDEFINED_X"
now defined

Warning: Constant UNDEFINED_X already defined, this will be an error in PHP 9 in %TEST% on line 37
"###
    );
}

/// `global $x` of a variable the file-scope code declares: aliases, unset, typed-property references, destructors, foreach.
#[test]
fn globals_in_file_scope_registers() {
    let out = run(r###"<?php
$g = 1;
function bump() { global $g; $g = $g + 1; return $g; }
for ($i = 0; $i < 5; $i++) { bump(); }
echo $g, "\n";
$alias = &$g;
for ($i = 0; $i < 5; $i++) { bump(); }
echo $g, " ", $alias, "\n";
$alias = 100;
echo bump(), " ", $g, "\n";
unset($g);
echo bump(), " ", isset($g) ? $g : "unset", " ", $alias, "\n";
function rebinds() { global $g; $local = &$g; $local = "str"; global $g; return $g; }
echo rebinds(), " ", $g, "\n";
function to_array() { global $g; $g = [1, 2]; return count($g); }
echo to_array(), " ", count($g), "\n";
function to_scalar() { global $g; $g = 5.5; return $g; }
echo to_scalar(), " ", $g, "\n";
class Box { public int $n = 1; public ?string $s = null; }
$box = new Box;
$typed = &$box->n;
function set_typed($v) { global $typed; $typed = $v; return $typed; }
var_dump(set_typed(7), set_typed("8"), $box->n);
try { set_typed("abc"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
var_dump($box->n);
class D { function __destruct() { echo "destruct\n"; } }
$obj = 1;
function put_obj() { global $obj; $obj = new D; echo "put\n"; }
function put_int() { global $obj; $obj = 3; echo "replaced\n"; }
put_obj(); put_int(); echo $obj, "\n";
function in_foreach() { global $list; foreach ([1, 2, 3] as $v) { $list = $v; } return $list; }
echo in_foreach(), "\n";
$arr = [1, 2, 3];
function over_ref() { global $arr; $n = 0; foreach ($arr as $v) { $n += $v; $arr = 9; } return $n; }
echo over_ref(), " ", $arr, "\n";
class Foo { public static int $n = 1; }
$sr = &Foo::$n;
function set_static($v) { global $sr; $sr = $v; return $sr; }
var_dump(set_static(5), set_static("6"));
try { set_static("x"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
function loop_static() { global $sr; for ($i = 0; $i < 3; $i++) { $sr = $i; } return Foo::$n; }
var_dump(loop_static(), Foo::$n);
"###);
    assert_eq!(
        out,
        r###"6
11 11
101 101
1 1 101
str str
2 2
5.5 5.5
int(7)
int(8)
int(8)
Cannot assign string to reference held by property Box::$n of type int
int(8)
put
destruct
replaced
3
3
6 9
int(5)
int(6)
Cannot assign string to reference held by property Foo::$n of type int
int(2)
int(2)
"###
    );
}

/// `global $x` of a variable that only lives in the globals table: `$GLOBALS` writes, unset, includes, `$$name`.
#[test]
fn globals_in_the_request_table() {
    let out = run(r###"<?php
function init() { global $LAST; $LAST = 42; }
function step() { global $LAST; $LAST = ($LAST * 3877 + 29573) % 139968; return $LAST; }
init();
$s = 0;
for ($i = 0; $i < 1000; $i++) { $s += step(); }
echo $s, " ", $GLOBALS['LAST'], "\n";
$GLOBALS['LAST'] = 7;
echo step(), "\n";
unset($GLOBALS['LAST']);
echo step(), " ", $GLOBALS['LAST'], "\n";
function other() { global $LAST; return $LAST; }
$copy = other();
function drop() { global $LAST; unset($LAST); return isset($LAST) ? "set" : "unset"; }
echo drop(), " ", other(), "\n";
function make_new() { global $FRESH; $FRESH = ($FRESH ?? 0) + 1; return $FRESH; }
for ($i = 0; $i < 5; $i++) { make_new(); }
echo make_new(), "\n";
$name = "FRESH";
$$name = 100;
echo make_new(), " ", $FRESH, "\n";
file_put_contents(__DIR__ . "/inc_g.php", '<?php $INC = 5; function inc_step() { global $INC; $INC = $INC + 1; return $INC; } for ($k = 0; $k < 3; $k++) { inc_step(); } echo $INC, "\n";');
include __DIR__ . "/inc_g.php";
echo inc_step(), " ", $INC, "\n";
function arr_global() { global $ARR; $ARR = [1]; $ARR[] = 2; return count($ARR); }
echo arr_global(), arr_global(), "\n";
var_dump(array_key_exists('LAST', $GLOBALS), $GLOBALS['FRESH']);
"###);
    assert_eq!(
        out,
        r###"72233348 11426
56712
29573 29573
unset 29573
6
101 101
8
9 9
22
bool(true)
int(101)
"###
    );
}

/// List loops that run long start in place, unbox from where they stopped, and stay in place on a hole or another type.
#[test]
fn list_loops_unbox_after_a_probe() {
    let out = run(r###"<?php
function heapsort_r($n, &$ra) {
    $l = ($n >> 1) + 1;
    $ir = $n;
    while (1) {
        if ($l > 1) {
            $rra = $ra[--$l];
        } else {
            $rra = $ra[$ir];
            $ra[$ir] = $ra[1];
            if (--$ir == 1) { $ra[1] = $rra; return; }
        }
        $i = $l;
        $j = $l << 1;
        while ($j <= $ir) {
            if (($j < $ir) && ($ra[$j] < $ra[$j+1])) { $j++; }
            if ($rra < $ra[$j]) { $ra[$i] = $ra[$j]; $j += ($i = $j); } else { $j = $ir + 1; }
        }
        $ra[$i] = $rra;
    }
}
function check($ra, $n) {
    $ok = true; $sum = 0;
    for ($i = 1; $i < $n; $i++) { if ($ra[$i] > $ra[$i + 1]) { $ok = false; } $sum += $ra[$i]; }
    return [$ok, $sum + $ra[$n]];
}
foreach ([10, 1000, 30000] as $n) {
    $LAST = 42;
    $f = []; $k = [];
    for ($i = 1; $i <= $n; $i++) { $LAST = ($LAST * 3877 + 29573) % 139968; $f[$i] = $LAST / 139968; $k[$i] = $LAST; }
    $copy = $k;
    heapsort_r($n, $f);
    heapsort_r($n, $k);
    [$okf, $sf] = check($f, $n);
    [$okk, $sk] = check($k, $n);
    printf("%d %s %.6f %.10f %s %d %d %d %d\n", $n, var_export($okf, true), $sf, $f[$n], var_export($okk, true), $sk, $k[1], $k[$n], $copy[1]);
}
// A long loop over an array that cannot be unboxed (a hole, then a string).
function sum_twice(array &$a, int $n) {
    $s = 0;
    for ($r = 0; $r < 50; $r++) { for ($i = 0; $i < $n; $i++) { $s += $a[$i]; $a[$i] = $a[$i] + 1; } }
    return $s;
}
$h = range(0, 999);
unset($h[500]);
try { echo sum_twice($h, 1000), "\n"; } catch (Throwable $e) { echo get_class($e), "\n"; }
echo count($h), " ", $h[499], " ", isset($h[500]) ? $h[500] : "-", " ", $h[501] ?? "-", "\n";
$m = range(0, 999);
$m[700] = "7";
echo sum_twice($m, 1000), " ", $m[700], " ", $m[999], "\n";
$z = range(0, 3999);
echo sum_twice($z, 4000), " ", $z[0], " ", $z[3999], "\n";
"###);
    assert_eq!(
        out,
        r###"10 true 4.660401 0.7934813672 true 652307 1167 111062 52439
1000 true 516.070445 0.9990640718 true 72233348 304 139837 52439
30000 true 15083.623628 0.9999857110 true 2111224632 0 139966 52439

Warning: Undefined array key 500 in %TEST% on line 41

Warning: Undefined array key 500 in %TEST% on line 41
26175000
1000 549 50 551
26165350 57 1049
404800000 50 4049
"###
    );
}

/// `echo` in Tier-2 loops collects strings into the output, around `ob_start()` callbacks, chunk sizes, nested buffers and other operands.
#[test]
fn echo_inside_tier_two() {
    let out = run(r###"<?php
function grid($w, $h) {
    for ($y = 0; $y < $h; $y++) {
        for ($x = 0; $x < $w; $x++) {
            $c = 0;
            $re = $x * 0.1; $im = $y * 0.1;
            while ($re * $re + $im * $im < 4.0 && $c < 50) { $t = $re * $re - $im * $im + 0.1; $im = 2 * $re * $im + 0.2; $re = $t; $c++; }
            if ($c == 50) { print "_"; } else { echo "#"; }
        }
        print "<br>";
    }
}
grid(30, 4); echo "\n";
ob_start(function ($buffer, $phase) { return "[" . strlen($buffer) . ":" . $phase . "]" . strtoupper($buffer); });
grid(20, 3);
ob_end_flush(); echo "\n";
ob_start(function ($buffer) { return "{" . $buffer . "}"; }, 7);
grid(10, 2);
ob_end_flush(); echo "\n";
ob_start();
ob_start();
grid(5, 2);
$inner = ob_get_clean();
grid(3, 1);
$outer = ob_get_clean();
echo strlen($inner), ":", $inner, "|", $outer, "\n";
function nums($n) { for ($i = 0; $i < $n; $i++) { echo $i; echo ","; } }
nums(20); echo "\n";
function mixed_out($n) { $s = "ab"; for ($i = 0; $i < $n; $i++) { echo $s; echo 1.5; echo $i % 2 ? "x" : "y"; } }
mixed_out(5); echo "\n";
function warns($n) { for ($i = 0; $i < $n; $i++) { echo "a"; echo $undefined ?? "-"; } }
warns(3); echo "\n";
ob_start();
function len_inside($n) { for ($i = 0; $i < $n; $i++) { echo "abc"; } return ob_get_length(); }
$l = len_inside(10);
ob_end_clean();
echo $l, "\n";
"###);
    assert_eq!(
        out,
        r###"_________#####################<br>_________#####################<br>_________#####################<br>________######################<br>
[72:9]_________###########<BR>_________###########<BR>_________###########<BR>
{_______}{__#<br>}{_______}{__#<br>}{}
18:_____<br>_____<br>|___<br>
0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,
ab1.5yab1.5xab1.5yab1.5xab1.5y
a-a-a-
30
"###
    );
}

/// `foreach ($a as $k => $v) $b[$k] += $v;` goes on in bulk after spending its budget, and stops exactly at a missing key, a float or an overflow.
#[test]
fn bulk_keyed_adds_resume_after_a_yield() {
    let out = run(r###"<?php
function hash2($n, $rounds) {
    for ($i = 0; $i < $n; $i++) { $hash1["foo_$i"] = $i; $hash2["foo_$i"] = 0; }
    for ($i = $rounds; $i > 0; $i--) {
        foreach ($hash1 as $key => $value) $hash2[$key] += $value;
    }
    $first = "foo_0"; $last = "foo_" . ($n - 1);
    return "$hash1[$first] $hash1[$last] $hash2[$first] $hash2[$last] " . array_sum($hash2) . " " . count($hash2);
}
echo hash2(500, 500), "\n";
echo hash2(70000, 5), "\n";
echo hash2(3, 2), "\n";
// Keys missing from the target part-way: the general path adds them with a warning.
function partial($n) {
    $src = []; $dst = [];
    for ($i = 0; $i < $n; $i++) { $src["k$i"] = $i; if ($i % 1000 != 999) { $dst["k$i"] = 1; } }
    foreach ($src as $key => $value) $dst[$key] += $value;
    foreach ($src as $key => $value) $dst[$key] += $value;
    return array_sum($dst) . " " . count($dst) . " " . $dst["k999"] . " " . array_key_last($dst);
}
echo @partial(300000), "\n";
// A value that is not an int part-way.
function floats($n) {
    $src = []; $dst = [];
    for ($i = 0; $i < $n; $i++) { $src["k$i"] = $i == 70000 ? 0.5 : $i; $dst["k$i"] = 0; }
    for ($r = 0; $r < 3; $r++) { foreach ($src as $key => $value) $dst[$key] += $value; }
    return array_sum($dst) . " " . var_export($dst["k70000"], true) . " " . $key . " " . $value;
}
echo floats(140000), "\n";
// The loop variables after the loop and an overflow on the way.
function overflow() {
    $src = ["a" => PHP_INT_MAX, "b" => 1]; $dst = ["a" => 1, "b" => 2];
    foreach ($src as $k => $v) $dst[$k] += $v;
    return var_export($dst, true) . " $k $v";
}
echo overflow(), "\n";
function simplecall() { for ($i = 0; $i < 100000; $i++) strlen("hallo"); return $i; }
echo simplecall(), "\n";
"###);
    assert_eq!(
        out,
        r###"0 499 0 249500 62375000 500
0 69999 0 349995 12249825000 70000
0 2 0 4 6 3
89999999700 300000 1998 k299999
29399580001.5 1.5 k139999 139999
array (
  'a' => 9.223372036854776E+18,
  'b' => 3,
) b 1
100000
"###
    );
}
