--TEST--
Increment overflow requires a float-capable property and reference
--FILE--
<?php
class Counter {
    public int|bool $value = PHP_INT_MAX;
    public static int $floor = PHP_INT_MIN;
    public int|float $wide = PHP_INT_MAX;
}
function attempt($change) {
    try { $change(); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
$c = new Counter;
attempt(fn() => $c->value++);
attempt(fn() => --Counter::$floor);
$list = [&$c->value];
attempt(function () use (&$list) { ++$list[0]; });
var_dump($c->value === PHP_INT_MAX, Counter::$floor === PHP_INT_MIN);
var_dump(is_float(++$c->wide));
attempt(fn() => $c->value += 1);
--EXPECT--
Cannot increment property Counter::$value of type int|bool past its maximal value
Cannot decrement property Counter::$floor of type int past its minimal value
Cannot increment a reference held by property Counter::$value of type int|bool past its maximal value
bool(true)
bool(true)
bool(true)
