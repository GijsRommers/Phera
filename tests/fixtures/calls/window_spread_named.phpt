--TEST--
Call window: spread named
--FILE--
<?php
function f($a=1,$b=2,...$rest){var_dump($a,$b,$rest);}f(...['b'=>8,'x'=>9]);
--EXPECT--
int(1)
int(8)
array(1) {
  ["x"]=>
  int(9)
}
