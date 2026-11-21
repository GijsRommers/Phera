--TEST--
Path keys that are locals are read in place: undefined-key warnings in every mode, evaluation order, exposed locals, loop back edges
--FILE--
<?php
error_reporting(E_ALL);
function read_both() { $x = $a[$k]; return $x; }
function read_nested() { $x = $a[$i][$j]; return $x; }
function read_prop() { $o = new stdClass; $x = $o->$p; return $x; }
function check_isset() { return isset($a[$k]); }
function check_coalesce() { return $a[$k] ?? 1; }
function check_empty() { return empty($a[$k]) ? 'e' : 'f'; }
function do_unset() { unset($a[$k]); return 'ok'; }
function write_both() { $a[$k] = $v; return 'ok'; }
function order_incdec() { $a = [[5]]; $i = 0; return $a[$i][$i++]; }
function order_assign() { $x = 1; return $x + ($x = 5); }
function order_key() { $arr = [1 => 10]; $j = 1; return $arr[$j] + ($j = 9); }
function loop_keys(array $rows, $key) { $sum = 0; foreach ($rows as $row) { $sum += $row[$key]; } return $sum; }
function ref_key() { $a = ['k' => 7]; $k = 'k'; $rk = &$k; return $a[$rk]; }
function byref_param(array $a, &$k) { return $a[$k]; }
function with_global(array $a) { global $gk; $k = $gk; return $a[$k]; }
function with_static(array $a) { static $k = 's'; return $a[$k]; }
function with_try(array $a, $k) { try { return $a[$k]; } catch (Throwable $e) { return 'caught'; } }
function gen(array $a, $k) { yield $a[$k]; yield $a[$k] ?? 'n'; }
function with_extract(array $a) { extract(['k' => 'e']); return $a[$k]; }
function coalesce_assign(array $a, $k) { $a[$k] ??= 5; return $a; }
function back_edge(array $a) { $k = 'a'; $sum = 0; while (true) { $sum += $a[$k]; if ($k === 'a') { $k = 'b'; } else { break; } } return $sum; }
function loop_write(array $a, $k) { for ($n = 0; $n < 2; $n++) { $a[$k] = $n; } return $a; }
$gk = 'g';
var_dump(read_both());
var_dump(read_nested());
var_dump(read_prop());
var_dump(check_isset());
var_dump(check_coalesce());
var_dump(check_empty());
var_dump(do_unset());
var_dump(write_both());
var_dump(order_incdec());
var_dump(order_assign());
var_dump(order_key());
var_dump(loop_keys([['n' => 1], ['n' => 2]], 'n'));
var_dump(ref_key());
$k = 'k'; var_dump(byref_param(['k' => 3], $k));
var_dump(with_global(['g' => 4]));
var_dump(with_static(['s' => 5]));
var_dump(with_try(['t' => 6], 't'));
foreach (gen(['q' => 8], 'q') as $v) var_dump($v);
var_dump(with_extract(['e' => 9]));
var_dump(coalesce_assign([], 'c'));
var_dump(back_edge(['a' => 1, 'b' => 2]));
var_dump(loop_write([], 'w'));
--EXPECTF--

Warning: Undefined variable $a in %s on line 3

Warning: Undefined variable $k in %s on line 3

Warning: Trying to access array offset on null in %s on line 3
NULL

Warning: Undefined variable $a in %s on line 4

Warning: Undefined variable $i in %s on line 4

Warning: Trying to access array offset on null in %s on line 4

Warning: Undefined variable $j in %s on line 4

Warning: Trying to access array offset on null in %s on line 4
NULL

Warning: Undefined variable $p in %s on line 5

Warning: Undefined property: stdClass::$ in %s on line 5
NULL

Warning: Undefined variable $k in %s on line 6
bool(false)

Warning: Undefined variable $k in %s on line 7
int(1)

Warning: Undefined variable $k in %s on line 8
string(1) "e"

Warning: Undefined variable $k in %s on line 9
string(2) "ok"

Warning: Undefined variable $v in %s on line 10

Warning: Undefined variable $k in %s on line 10

Deprecated: Using null as an array offset is deprecated, use an empty string instead in %s on line 10
string(2) "ok"
int(5)
int(10)
int(19)
int(3)
int(7)
int(3)
int(4)
int(5)
int(6)
int(8)
int(8)
int(9)
array(1) {
  ["c"]=>
  int(5)
}
int(3)
array(1) {
  ["w"]=>
  int(1)
}
