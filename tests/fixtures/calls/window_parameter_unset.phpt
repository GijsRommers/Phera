--TEST--
Call window: parameter unset
--FILE--
<?php
function f($a){unset($a);var_dump(func_get_args());$a=7;var_dump(func_get_args());}$x=4;f($x);var_dump($x);
--EXPECT--
array(1) {
  [0]=>
  NULL
}
array(1) {
  [0]=>
  int(7)
}
int(4)
