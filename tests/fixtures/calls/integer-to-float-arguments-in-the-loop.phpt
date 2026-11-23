--TEST--
Integers sent to float parameters become floats, also in strict mode
--FILE--
<?php
declare(strict_types=1);
class Vec { public function __construct(public float $x, public ?float $y = null) {} }
function half(float $value): float { return $value / 2; }
function either(int|float $value): string { return get_debug_type($value); }
function kind(float $value): string { return get_debug_type($value) . ' ' . var_export($value, true); }
for ($round = 0; $round < 3; $round++) {
    $v = new Vec($round, $round + 1);
    echo var_export($v->x, true), ' ', var_export($v->y, true), ' ', half($round + 1), ' ', either($round), ' ', kind(PHP_INT_MAX), ' ', kind(-3), "\n";
    try {
        half("1.5");
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
--EXPECTF--
0.0 1.0 0.5 int float 9.223372036854776E+18 float -3.0
half(): Argument #1 ($value) must be of type float, string given, called in %s on line 11
1.0 2.0 1 int float 9.223372036854776E+18 float -3.0
half(): Argument #1 ($value) must be of type float, string given, called in %s on line 11
2.0 3.0 1.5 int float 9.223372036854776E+18 float -3.0
half(): Argument #1 ($value) must be of type float, string given, called in %s on line 11
