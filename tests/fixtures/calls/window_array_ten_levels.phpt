--TEST--
Call window: array ten levels
--FILE--
<?php
function f($x,$n){if($n)return f($x,$n-1);$x['a']=9;return $x;}$x=['a'=>1];var_dump(f($x,10),$x);
--EXPECT--
array(1) {
  ["a"]=>
  int(9)
}
array(1) {
  ["a"]=>
  int(1)
}
