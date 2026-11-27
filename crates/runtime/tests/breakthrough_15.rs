//! Breakthrough 15 regression tests, checked against PHP 8.5 (`php -n`):
//! - Tier-2 list loops pick their element layout per entry (the array's own
//!   slots, or an unboxed copy), fall back exactly at a hole, another element
//!   type or a reference, and read a shared array without separating it
//! - String-keyed array writes probe the index once; the index keeps 32 bits
//!   of each key's hash, so growth never hashes a key again
//! - `range()` builds integer ranges without per-element float checks

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

/// A list loop entered thousands of times (binary search), fills that meet
/// another type, a hole or a reference element part-way through, sorts that
/// run unboxed, and copy-on-write around readers and writers.
#[test]
fn list_loops_run_in_place_with_exact_fallbacks() {
    let out = run(r###"<?php
function bsearch(array $a, int $x): int {
    $lo = 0; $hi = count($a) - 1;
    while ($lo <= $hi) {
        $mid = ($lo + $hi) >> 1;
        if ($a[$mid] < $x) { $lo = $mid + 1; } else { $hi = $mid - 1; }
    }
    return $lo;
}
$a = range(0, 20000, 2);
$s = 0;
for ($i = 0; $i < 3000; $i++) { $s += bsearch($a, $i * 7); }
echo $s, "\n";

function fill(array $f): array {
    for ($i = 2; $i < 40; $i++) {
        if ($f[$i] > 0) {
            for ($k = $i + $i; $k <= 60; $k += $i) { $f[$k] = 0; }
        }
    }
    return $f;
}
$f = range(0, 60);
$f[30] = 1.5;           // another type in the middle
echo implode(",", fill($f)), "\n";
$g = range(0, 60);
unset($g[45]);          // a hole in the middle
echo implode(",", fill($g)), "|", count(fill($g)), "\n";
$h = range(0, 60);
$x = 7; $h[24] = &$x;   // a reference element
$out = fill($h);
echo implode(",", $out), "|", $x, "\n";

function sift(array &$ra, int $n): void {
    for ($i = 1; $i < $n; $i++) {
        $j = $i;
        while ($j > 0 && $ra[$j - 1] > $ra[$j]) {
            $t = $ra[$j]; $ra[$j] = $ra[$j - 1]; $ra[$j - 1] = $t; $j--;
        }
    }
}
$v = [];
for ($i = 0; $i < 300; $i++) { $v[] = ($i * 7919) % 301; }
$w = $v;
sift($v, 300);
echo $v[0], " ", $v[150], " ", $v[299], " ", $w[0], " ", $w[1], "\n";
$fl = [];
for ($i = 0; $i < 50; $i++) { $fl[] = (($i * 37) % 50) / 4; }
sift($fl, 50);
echo implode(",", array_slice($fl, 0, 8)), "\n";
$orig = range(0, 60);
$filled = fill($orig);
echo $orig[4], " ", $filled[4], " ", count($orig), "\n";
$shared = range(0, 100, 5);
$alias = $shared;
echo bsearch($shared, 42), " ";
$alias[3] = 1000;
echo bsearch($shared, 16), " ", bsearch($alias, 16), " ", $shared[3], "\n";
"###);
    assert_eq!(
        out,
        r###"15710142
0,1,2,3,0,5,0,7,0,0,0,11,0,13,0,0,0,17,0,19,0,0,0,23,0,0,0,0,0,29,0,31,0,0,0,0,0,37,0,0,0,41,0,43,0,0,0,47,0,0,0,0,0,53,0,0,0,0,0,59,0
0,1,2,3,0,5,0,7,0,0,0,11,0,13,0,0,0,17,0,19,0,0,0,23,0,0,0,0,0,29,0,31,0,0,0,0,0,37,0,0,0,41,0,43,0,0,47,0,0,0,0,0,53,0,0,0,0,0,59,0,0|61
0,1,2,3,0,5,0,7,0,0,0,11,0,13,0,0,0,17,0,19,0,0,0,23,0,0,0,0,0,29,0,31,0,0,0,0,0,37,0,0,0,41,0,43,0,0,0,47,0,0,0,0,0,53,0,0,0,0,0,59,0|0
0 150 300 0 93
0,0.25,0.5,0.75,1,1.25,1.5,1.75
4 0 61
9 4 3 15
"###
    );
}

/// String keys that index grows past many rehashes, numeric-string
/// normalisation, unset holes, a reference element and copy-on-write.
#[test]
fn string_keys_insert_once_and_survive_growth() {
    let out = run(r###"<?php
$a = [];
for ($i = 1; $i <= 3000; $i++) { $a[dechex($i)] = $i; }
$c = 0;
for ($i = 3000; $i > 0; $i--) { if ($a[dechex($i)] === $i) { $c++; } }
echo $c, " ", count($a), " ", array_key_first($a), " ", array_key_last($a), "\n";
var_dump(isset($a["10"]), isset($a[16]), isset($a["010"]), array_keys($a)[15]);
for ($i = 1; $i <= 3000; $i += 3) { unset($a[dechex($i)]); }
for ($i = 1; $i <= 3000; $i += 6) { $a[dechex($i)] = -$i; }
$s = 0; foreach ($a as $k => $v) { $s += $v; }
echo count($a), " ", $s, " ", array_key_last($a), "\n";
$b = ['x' => 1];
for ($i = 0; $i < 20; $i++) { $b["k$i"] = $i; }
$r = &$b['k5'];
$name = "k" . 5;
$b[$name] = 99;
echo $r, " ", $b['k5'], "\n";
$b["-0"] = 'neg zero'; $b["-5"] = 'neg five'; $b["9223372036854775808"] = 'big';
var_dump(array_slice(array_keys($b), -3));
$copy = $b; $copy["k1"] = 'changed';
echo $b["k1"], " ", $copy["k1"], "\n";
"###);
    assert_eq!(
        out,
        r###"3000 3000 1 bb8
bool(true)
bool(true)
bool(false)
int(10)
2500 2253000 bb3
99 99
array(3) {
  [0]=>
  string(2) "-0"
  [1]=>
  int(-5)
  [2]=>
  string(19) "9223372036854775808"
}
1 changed
"###
    );
}

/// `range()` integer fast path at both ends of `int`, with steps, descending,
/// and the character and float kinds left to the general path.
#[test]
fn integer_ranges_at_the_edges_of_int() {
    let out = run(r###"<?php
echo implode(",", range(0, 10, 3)), "\n";
echo implode(",", range(5, 1, 2)), "\n";
echo implode(",", range(-3, 3)), "\n";
echo implode(",", range(PHP_INT_MAX - 2, PHP_INT_MAX)), "\n";
echo implode(",", range(PHP_INT_MIN, PHP_INT_MIN + 4, 2)), "\n";
echo implode(",", range(PHP_INT_MAX, PHP_INT_MAX - 4, 2)), "\n";
echo implode(",", range(7, 7)), "\n";
echo implode(",", range(10, 0, -5)), "\n";
var_dump(range(1, 3) === [1, 2, 3], range("1", "3"), range(1, 2, 0.5));
echo implode(",", range('a', 'e', 2)), "\n";
"###);
    assert_eq!(
        out,
        r###"0,3,6,9
5,3,1
-3,-2,-1,0,1,2,3
9223372036854775805,9223372036854775806,9223372036854775807
-9223372036854775808,-9223372036854775806,-9223372036854775804
9223372036854775807,9223372036854775805,9223372036854775803
7
10,5,0
bool(true)
array(3) {
  [0]=>
  string(1) "1"
  [1]=>
  string(1) "2"
  [2]=>
  string(1) "3"
}
array(3) {
  [0]=>
  float(1)
  [1]=>
  float(1.5)
  [2]=>
  float(2)
}
a,c,e
"###
    );
}
