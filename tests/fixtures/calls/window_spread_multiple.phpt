--TEST--
Call window: spread multiple
--FILE--
<?php
function f($a,$b,...$rest){var_dump($a,$b,$rest);}f(...[1],...[2,3],x:4);
--EXPECT--
int(1)
int(2)
array(2) {
  [0]=>
  int(3)
  ["x"]=>
  int(4)
}
