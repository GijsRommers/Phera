--TEST--
shuffle renumbers a self-referential array and preserves the reference
--FILE--
<?php
mt_srand(42);
$a = [2, 1];
$a[5] = &$a;
var_dump(shuffle($a), array_keys($a), key($a), in_array($a, $a, true));
--EXPECT--
bool(true)
array(3) {
  [0]=>
  int(0)
  [1]=>
  int(1)
  [2]=>
  int(2)
}
int(0)
bool(true)
