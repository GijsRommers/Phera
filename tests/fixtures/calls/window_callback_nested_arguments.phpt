--TEST--
Call window: callback nested arguments
--FILE--
<?php
function f($a,$b){return $a+$b;}var_dump(array_map(fn($v)=>f($v,f(1,2)),[3,4]));
--EXPECT--
array(2) {
  [0]=>
  int(6)
  [1]=>
  int(7)
}
