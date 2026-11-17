--TEST--
Call window: extra recursive
--FILE--
<?php
function f($n){if($n)f($n-1,'inner',[2]);var_dump(func_get_args());}f(1,'outer',3);
--EXPECT--
array(3) {
  [0]=>
  int(0)
  [1]=>
  string(5) "inner"
  [2]=>
  array(1) {
    [0]=>
    int(2)
  }
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  string(5) "outer"
  [2]=>
  int(3)
}
