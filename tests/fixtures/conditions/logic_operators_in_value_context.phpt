--TEST--
logic_operators_in_value_context
--FILE--
<?php
// A comparison that is the left operand of a value-context || or && must
// still write its result where the jump lands on it (Laravel's isBuildable:
// `$concrete === $abstract || $concrete instanceof Closure`).
function isBuildable($concrete, $abstract) {
    return $concrete === $abstract || $concrete instanceof Closure;
}
$ab = 'Some\\Class';
var_dump(isBuildable($ab, $ab));
var_dump(isBuildable(function () {}, $ab));
var_dump(isBuildable($ab, 'Other'));

$a = 'x'; $b = 'x';
var_dump($a === $b);
var_dump(($a === $b) || false);
var_dump($a === $b || false);
var_dump(false || ($a === $b));
var_dump(($a === $b) && true);
var_dump($a !== $b && true);
var_dump(($a !== $b) || ($a === $b));
$i = 5; $j = 5;
var_dump(($i === $j) || false);
var_dump(($i !== $j) && false);
$n = null;
var_dump(($n === null) || false);
$f = 1.5;
var_dump(($f === 1.5) || false);
$ternary = ($a === $b) || ($i === $j) ? 'T' : 'F';
var_dump($ternary);
--EXPECTF--
bool(true)
bool(true)
bool(false)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
bool(true)
bool(true)
string(1) "T"
