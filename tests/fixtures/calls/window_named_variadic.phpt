--TEST--
Call window: named variadic
--FILE--
<?php
function f($a=1,...$rest){var_dump($a,$rest,func_get_args());}f(z:9,b:8);
--EXPECT--
int(1)
array(2) {
  ["z"]=>
  int(9)
  ["b"]=>
  int(8)
}
array(0) {
}
