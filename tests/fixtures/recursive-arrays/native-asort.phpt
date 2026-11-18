--TEST--
asort preserves the reference in a self-referential input
--FILE--
<?php
$a = [2, 1];
$a[5] = &$a;
var_dump(asort($a), $a[5] === $a, array_keys($a), key($a));
--EXPECT--
bool(true)
bool(true)
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(0)
  [2]=>
  int(5)
}
int(1)
