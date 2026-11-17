--TEST--
Call window: spread list
--FILE--
<?php
function f($a,$b,...$rest){var_dump($a,$b,$rest);}f(...[1,2,3,4]);
--EXPECT--
int(1)
int(2)
array(2) {
  [0]=>
  int(3)
  [1]=>
  int(4)
}
