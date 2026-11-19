--TEST--
Union types in type errors are printed in PHP's order
--FILE--
<?php
function f(int|string $x) {} try { f([]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
function g(null|float|bool|int|array|string|Countable|Stringable $x) {} try { g(new stdClass); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
class C { public int|string $p; public null|false|float|iterable $q; } $c = new C; try { $c->p = []; } catch (TypeError $e) { echo $e->getMessage(), "\n"; } try { $c->q = 'x'; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
function h(): float|int|null { return 'x'; } try { h(); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo (new ReflectionFunction('g'))->getParameters()[0]->getType(), "\n";
--EXPECTF--
f(): Argument #1 ($x) must be of type string|int, array given, called in %s on line %d
g(): Argument #1 ($x) must be of type Countable|Stringable|array|string|int|float|bool|null, stdClass given, called in %s on line %d
Cannot assign array to property C::$p of type string|int
Cannot assign string to property C::$q of type Traversable|array|float|false|null
h(): Return value must be of type int|float|null, string returned
Countable|Stringable|array|string|int|float|bool|null
