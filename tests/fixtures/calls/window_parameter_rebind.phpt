--TEST--
Call window: parameter rebind
--FILE--
<?php
function f(&$a){$b=8;$a=&$b;$a++;var_dump(func_get_args());}$x=4;f($x);var_dump($x);
--EXPECT--
array(1) {
  [0]=>
  int(9)
}
int(4)
