--TEST--
Call window: parameter assign
--FILE--
<?php
function f($a){$a=[7];var_dump(func_get_args());}$x=4;f($x);var_dump($x);
--EXPECT--
array(1) {
  [0]=>
  array(1) {
    [0]=>
    int(7)
  }
}
int(4)
