--TEST--
Call window: named skip default
--FILE--
<?php
function f($a=1,$b=2,$c=3){var_dump($a,$b,$c,func_get_args());}f(c:9);
--EXPECT--
int(1)
int(2)
int(9)
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(9)
}
