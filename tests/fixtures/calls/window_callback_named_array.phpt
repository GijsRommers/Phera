--TEST--
Call window: callback named array
--FILE--
<?php
function f($a=1,$b=2,...$rest){var_dump($a,$b,$rest);}call_user_func_array('f',['b'=>4,'x'=>5]);
--EXPECT--
int(1)
int(4)
array(1) {
  ["x"]=>
  int(5)
}
