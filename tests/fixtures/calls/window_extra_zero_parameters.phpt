--TEST--
Call window: extra zero parameters
--FILE--
<?php
function f(){var_dump(func_num_args(),func_get_args());}f('a',2,[3],new stdClass);
--EXPECT--
int(4)
array(4) {
  [0]=>
  string(1) "a"
  [1]=>
  int(2)
  [2]=>
  array(1) {
    [0]=>
    int(3)
  }
  [3]=>
  object(stdClass)#1 (0) {
  }
}
