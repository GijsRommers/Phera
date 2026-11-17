--TEST--
Call window: parameter reference unset
--FILE--
<?php
function f(&$a){unset($a);$a=9;var_dump(func_get_args());}$x=4;f($x);var_dump($x);
--EXPECT--
array(1) {
  [0]=>
  int(9)
}
int(4)
