--TEST--
array-keys-strict compares shared recursive arrays
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = $a;
var_dump(array_keys(['item' => $b], $a, true));
--EXPECT--
array(1) {
  [0]=>
  string(4) "item"
}
