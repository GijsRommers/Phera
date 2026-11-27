//! Breakthrough 13 regression tests:
//! - Offset `Storage::List` parity (1-indexed, 2-indexed, negative-indexed, `array_is_list`,
//!   `unset()` holes + `isset()`, re-insertion order, `array_pop`/`array_shift`)
//! - Bottom-tested loop rotation (`for`, `while`, 0 iterations, `break`, `continue`,
//!   comma-separated `for` conditions, owning-temp conditions destructor timing)
//! - Direct ternary return (`return $cond ? $a : $b;`, nested ternaries, owning temp in cond)
//! - `ReturnNull` and empty-body direct call shortcut (void, nullable, guarded destructors)
//! - `direct_operands` index `Move` folding edge cases

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
fn offset_list_parity_and_order() {
    let out = run(r#"<?php
$one = [];
for ($i = 1; $i <= 5; $i++) {
    $one[$i] = $i * 10;
}
var_dump(array_is_list($one), count($one), isset($one[0]), isset($one[1]), isset($one[5]), isset($one[6]));
echo implode(",", array_keys($one)) . "|" . implode(",", $one) . "\n";

// Unsetting and re-inserting an integer key moves it to the end (exact PHP semantics).
unset($one[2]);
$one[2] = 999;
echo implode(",", array_keys($one)) . "|" . implode(",", $one) . "\n";

// Negative-indexed dense array starts at -3.
$neg = [];
for ($i = -3; $i <= 1; $i++) {
    $neg[$i] = $i + 100;
}
var_dump(array_is_list($neg), count($neg));
echo implode(",", array_keys($neg)) . "|" . implode(",", $neg) . "\n";

// Pop and shift on offset list.
$s = [];
for ($i = 2; $i <= 5; $i++) {
    $s[$i] = $i;
}
$popped = array_pop($s);
$shifted = array_shift($s);
echo "$popped,$shifted|" . implode(",", array_keys($s)) . "|" . implode(",", $s) . "\n";
var_dump(array_is_list($s));

// Single-probe integer update preserves references.
$r = 42;
$arr = [1 => 10, 2 => &$r];
$arr[2] = 77;
echo "$r," . $arr[2] . "\n";
"#);
    assert_eq!(
        out,
        "bool(false)\nint(5)\nbool(false)\nbool(true)\nbool(true)\nbool(false)\n\
1,2,3,4,5|10,20,30,40,50\n\
1,3,4,5,2|10,30,40,50,999\n\
bool(false)\nint(5)\n\
-3,-2,-1,0,1|97,98,99,100,101\n\
5,2|0,1|3,4\n\
bool(true)\n\
77,77\n"
    );
}

#[test]
fn rotated_loops_edge_cases_and_destructor_order() {
    let out = run(r#"<?php
// Zero-iteration for and while.
$hits = 0;
for ($i = 10; $i < 5; $i++) { $hits++; }
while ($hits > 0) { $hits++; }
echo "zero:$hits\n";

// Break, continue, and comma-separated for condition.
$sum = 0;
for ($i = 0, $j = 10; $i++, $i <= 6; $j--) {
    if ($i === 2) continue;
    if ($i === 5) break;
    $sum += $i * 100 + $j;
}
echo "comma:$sum\n";

// Owning temporary in while/for condition must destroy before body and before exit.
class Cond {
    public function __construct(public string $id, public bool $ok) {
        echo "new:{$this->id}\n";
    }
    public function __destruct() {
        echo "del:{$this->id}\n";
    }
}
function check_cond(Cond $c): bool {
    return $c->ok;
}
$n = 0;
while (check_cond(new Cond("w$n", $n < 2))) {
    echo "body:$n\n";
    $n++;
}
echo "after-while\n";

for ($k = 0; check_cond(new Cond("f$k", $k < 2)); $k++) {
    echo "fbody:$k\n";
}
echo "after-for\n";
"#);
    assert_eq!(
        out,
        "zero:0\n\
comma:825\n\
new:w0\ndel:w0\nbody:0\n\
new:w1\ndel:w1\nbody:1\n\
new:w2\ndel:w2\nafter-while\n\
new:f0\ndel:f0\nfbody:0\n\
new:f1\ndel:f1\nfbody:1\n\
new:f2\ndel:f2\nafter-for\n"
    );
}

#[test]
fn direct_ternary_return_and_recursion_parity() {
    let out = run(r#"<?php
function fibo_r($n) {
    return ($n < 2) ? 1 : fibo_r($n - 2) + fibo_r($n - 1);
}
function nested($x) {
    return $x < 0 ? -1 : ($x === 0 ? 0 : 1);
}
class Flag {
    public function __construct(public string $tag, public bool $v) {}
    public function __destruct() { echo "drop:{$this->tag}\n"; }
}
function pick_with_temp(bool $v): string {
    return (new Flag("cond", $v))->v ? "yes" : "no";
}

echo fibo_r(10) . "," . nested(-5) . "," . nested(0) . "," . nested(7) . "\n";
echo pick_with_temp(true) . "\n";
echo pick_with_temp(false) . "\n";
"#);
    assert_eq!(out, "89,-1,0,1\ndrop:cond\nyes\ndrop:cond\nno\n");
}

#[test]
fn empty_body_direct_call_and_return_null_parity() {
    let out = run(r#"<?php
function noop($a) {}
function explicit_null() { return; }
function typed_nullable(): ?int { return null; }
function typed_void(int $x): void {}
function bad_return(): int {}

class Arg {
    public function __construct(public string $id) {}
    public function __destruct() { echo "drop:{$this->id}\n"; }
}

var_dump(noop(123), explicit_null(), typed_nullable(), typed_void(5));
noop(new Arg("a1"));
echo "after-a1\n";

try {
    typed_void("nope");
    echo "missed-arg-type\n";
} catch (TypeError $e) {
    echo "arg-type-error\n";
}

try {
    bad_return();
    echo "missed-ret-type\n";
} catch (TypeError $e) {
    echo "ret-type-error\n";
}
"#);
    assert_eq!(
        out,
        "NULL\nNULL\nNULL\nNULL\ndrop:a1\nafter-a1\narg-type-error\nret-type-error\n"
    );
}

#[test]
fn index_move_folding_edge_cases() {
    let out = run(r#"<?php
$a = [10, 20, 30];
$i = 1;
$x = $a[$i];
$a[$i] += 5;
$a[$i]++;
$j =& $i;
$j = 2;
$y = $a[$i];
unset($a[$i]);
echo "$x,$y," . $a[1] . "," . (isset($a[2]) ? "yes" : "no") . "\n";

// Self-indexed mutation where base and index share a variable name indirectly.
$k = 0;
$m = [0 => 5, 1 => 9];
$m[$k] += $m[$k];
echo $m[0] . "\n";
"#);
    assert_eq!(out, "20,30,26,no\n10\n");
}
