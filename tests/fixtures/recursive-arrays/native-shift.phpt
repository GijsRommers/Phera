--TEST--
Self-referential array passed to shift
--FILE--
<?php
$a = [];
$a[5] = &$a;
$a[] = 2;
$shifted = array_shift($a);
$a[] = 3;
var_dump($shifted, $a, key($a));
--EXPECT--
array(1) {
  [0]=>
  int(2)
}
array(2) {
  [0]=>
  int(2)
  [1]=>
  int(3)
}
int(0)
