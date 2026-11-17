--TEST--
Call window: extra one parameter
--FILE--
<?php
function f($x){$x='changed';var_dump(func_num_args(),func_get_args(),func_get_arg(2));}f('a',[2],(object)['v'=>3]);
--EXPECT--
int(3)
array(3) {
  [0]=>
  string(7) "changed"
  [1]=>
  array(1) {
    [0]=>
    int(2)
  }
  [2]=>
  object(stdClass)#1 (1) {
    ["v"]=>
    int(3)
  }
}
object(stdClass)#1 (1) {
  ["v"]=>
  int(3)
}
