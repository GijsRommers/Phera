--TEST--
Self-referential array passed to pop
--FILE--
<?php
$a = [1, 2];
$a[5] = &$a;
$popped = array_pop($a);
$a[] = 3;
var_dump($popped, $a, key($a));
--EXPECT--
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [5]=>
  int(3)
}
int(0)
