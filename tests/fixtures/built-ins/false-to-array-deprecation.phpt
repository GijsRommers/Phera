--TEST--
false written into as an array, in every way a write can reach it (a local, a nested level, append, ??=, a reference, a loop, a property, a typed property, a static, the top level, an element reference): PHP converts it with a deprecation reported once, before the key's own conversion; null converts silently.
--FILE--
<?php
// `false` written into as an array: PHP converts it with a deprecation, in
// every way a write can reach it; `null` converts silently.

class Box { public $f = false; public static $s = false; public array|false $typed = false; }

function local_writes() {
    $f = false; $f['k'] = 1; var_dump($f);
    $f = false; $f[] = 'append'; var_dump($f);
    $f = false; $f['a']['b'] = 'nested'; var_dump($f);
    $f = false; $f['k'] ??= 'coalesce'; var_dump($f);
    $f = false; $r = &$f; $r['ref'] = 1; var_dump($f);
    $n = null; $n['k'] = 'null is silent'; var_dump($n);
    $f = false; unset($f['k']); var_dump($f);
    for ($i = 0; $i < 3; $i++) { $g = false; $g[$i] = $i; }
    var_dump($g);
}
local_writes();

$b = new Box;
$b->f['k'] = 1; var_dump($b->f);
$b->typed['k'] = 1; var_dump($b->typed);
Box::$s['k'] = 1; var_dump(Box::$s);
$top = false; $top['k'] = 1; var_dump($top);
class RefBox { public $f = false; }
$rb = new RefBox;
$alias = &$rb->f['k']; $alias = 'through a reference'; var_dump($rb->f);
function local_reference() { $f = false; $alias = &$f['k']; $alias = 'local reference'; var_dump($f); }
local_reference();
// The level is reported before its key's own conversion.
function float_key() { $f = false; $f[1.5] = 'float key'; var_dump($f); }
float_key();
$order = new RefBox; $order->f[2.5] = 'float key'; var_dump($order->f);

--EXPECTF--

Deprecated: Automatic conversion of false to array is deprecated in %s on line 8
array(1) {
  ["k"]=>
  int(1)
}

Deprecated: Automatic conversion of false to array is deprecated in %s on line 9
array(1) {
  [0]=>
  string(6) "append"
}

Deprecated: Automatic conversion of false to array is deprecated in %s on line 10
array(1) {
  ["a"]=>
  array(1) {
    ["b"]=>
    string(6) "nested"
  }
}

Deprecated: Automatic conversion of false to array is deprecated in %s on line 11
array(1) {
  ["k"]=>
  string(8) "coalesce"
}

Deprecated: Automatic conversion of false to array is deprecated in %s on line 12
array(1) {
  ["ref"]=>
  int(1)
}
array(1) {
  ["k"]=>
  string(14) "null is silent"
}

Deprecated: Automatic conversion of false to array is deprecated in %s on line 14
bool(false)

Deprecated: Automatic conversion of false to array is deprecated in %s on line 15

Deprecated: Automatic conversion of false to array is deprecated in %s on line 15

Deprecated: Automatic conversion of false to array is deprecated in %s on line 15
array(1) {
  [2]=>
  int(2)
}

Deprecated: Automatic conversion of false to array is deprecated in %s on line 21
array(1) {
  ["k"]=>
  int(1)
}

Deprecated: Automatic conversion of false to array is deprecated in %s on line 22
array(1) {
  ["k"]=>
  int(1)
}

Deprecated: Automatic conversion of false to array is deprecated in %s on line 23
array(1) {
  ["k"]=>
  int(1)
}

Deprecated: Automatic conversion of false to array is deprecated in %s on line 24
array(1) {
  ["k"]=>
  int(1)
}

Deprecated: Automatic conversion of false to array is deprecated in %s on line 27
array(1) {
  ["k"]=>
  &string(19) "through a reference"
}

Deprecated: Automatic conversion of false to array is deprecated in %s on line 28
array(1) {
  ["k"]=>
  &string(15) "local reference"
}

Deprecated: Automatic conversion of false to array is deprecated in %s on line 31

Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 31
array(1) {
  [1]=>
  string(9) "float key"
}

Deprecated: Automatic conversion of false to array is deprecated in %s on line 33

Deprecated: Implicit conversion from float 2.5 to int loses precision in %s on line 33
array(1) {
  [2]=>
  string(9) "float key"
}
