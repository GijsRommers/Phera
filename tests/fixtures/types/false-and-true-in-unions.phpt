--TEST--
false and true in a union accept that one value; nothing is converted to a boolean for them
--FILE--
<?php
class C { public null|false|float|iterable $q; public int|false $r; public string|false $s; public true|int $t; }
$c = new C;
foreach (['q' => ['x', '1.5', 1, true, false, 0, '', [], null], 'r' => ['x', '5', true, false, 0.0, 1.5, null, ''], 's' => [true, false, 5, null], 't' => [true, false, 'x', '1', 1.0, 0]] as $prop => $values) {
    foreach ($values as $v) { try { $c->$prop = $v; echo $prop, ' <- ', json_encode($v), ' = ', var_export($c->$prop, true), "\n"; } catch (TypeError $e) { echo $prop, ' <- ', json_encode($v), ' TypeError: ', $e->getMessage(), "\n"; } }
}
function f(int|false $a, null|false|float $b = null) { return var_export($a, true) . ' ' . var_export($b, true); }
foreach ([['x'], [true], ['7'], [1, 'x'], [1, true], [1, '2.5'], [false, false]] as $args) { try { echo f(...$args), "\n"; } catch (TypeError $e) { echo 'TypeError: ', preg_replace('/, called in.*/', '', $e->getMessage()), "\n"; } }
--EXPECTF--
q <- "x" TypeError: Cannot assign string to property C::$q of type Traversable|array|float|false|null
q <- "1.5" = 1.5
q <- 1 = 1.0
q <- true = 1.0
q <- false = false
q <- 0 = 0.0
q <- "" TypeError: Cannot assign string to property C::$q of type Traversable|array|float|false|null
q <- [] = array (
)
q <- null = NULL
r <- "x" TypeError: Cannot assign string to property C::$r of type int|false
r <- "5" = 5
r <- true = 1
r <- false = false
r <- 0 = 0

Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line %d
r <- 1.5 = 1
r <- null TypeError: Cannot assign null to property C::$r of type int|false
r <- "" TypeError: Cannot assign string to property C::$r of type int|false
s <- true = '1'
s <- false = false
s <- 5 = '5'
s <- null TypeError: Cannot assign null to property C::$s of type string|false
t <- true = true
t <- false = 0
t <- "x" TypeError: Cannot assign string to property C::$t of type int|true
t <- "1" = 1
t <- 1 = 1
t <- 0 = 0
TypeError: f(): Argument #1 ($a) must be of type int|false, string given
1 NULL
7 NULL
TypeError: f(): Argument #2 ($b) must be of type float|false|null, string given
1 1.0
1 2.5
false false
