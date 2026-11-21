--TEST--
Operator operands that are locals are read in place: undefined-operand warnings, casts, unary/binary ops, evaluation order, reference locals
--FILE--
<?php
error_reporting(E_ALL);
$log = [];
set_error_handler(function ($no, $str) use (&$log) { $log[] = "warn: $str"; return true; });
function f() { global $log; $log[] = 'F'; return 1; }
function c1() { return (int)$u; }
function c2() { return (bool)$u; }
function c3() { return -$u; }
function c4() { return !$u; }
function c5() { return ~5; }
function c6() { return $u + 1; }
function c7() { return 1 + $u; }
function c8() { return $u . 'x'; }
function c9($x) { return $x + ($x = 5); }
function c11($a, $k) { return (int)$a[$k]; }
function c12($x) { $r = &$x; return -(int)$r; }
function c13() { return (string)$u; }
function c14($x) { return -$x + f(); }
c1(); c2(); c3(); c4(); c5(); c6(); c7(); c8(); c9(1); c11(['a'=>1], 'a'); c12(3); c13(); c14(7);
foreach ($log as $l) echo $l, "\n";
var_dump(c1(), c2(), c3(), c4(), c5(), c6(), c7(), c8(), c9(1), c12(3), c14(7));
--EXPECT--
warn: Undefined variable $u
warn: Undefined variable $u
warn: Undefined variable $u
warn: Undefined variable $u
warn: Undefined variable $u
warn: Undefined variable $u
warn: Undefined variable $u
warn: Undefined variable $u
F
int(0)
bool(false)
int(0)
bool(true)
int(-6)
int(1)
int(1)
string(1) "x"
int(10)
int(-3)
int(-6)
