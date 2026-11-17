--TEST--
Call window: extra nested call
--FILE--
<?php
function f($a){return func_get_args();}function g(){return 'inner';}var_dump(f(1,g(),[3],false));
--EXPECT--
array(4) {
  [0]=>
  int(1)
  [1]=>
  string(5) "inner"
  [2]=>
  array(1) {
    [0]=>
    int(3)
  }
  [3]=>
  bool(false)
}
