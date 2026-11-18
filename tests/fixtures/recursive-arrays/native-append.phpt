--TEST--
Appending a self-reference returns the array after the append
--FILE--
<?php
$a = [1];
$result = ($a[] = &$a);
var_dump(array_keys($a), $a[1] === $a, $result === $a);
--EXPECT--
array(2) {
  [0]=>
  int(0)
  [1]=>
  int(1)
}
bool(true)
bool(true)
