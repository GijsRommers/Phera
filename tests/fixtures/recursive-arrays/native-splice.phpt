--TEST--
Self-referential array passed to splice
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$removed = array_splice($a, 1, 1);
$a[] = 42;
var_dump($removed[0], key($a));
--EXPECT--
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(42)
}
int(0)
