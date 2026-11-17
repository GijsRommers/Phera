--TEST--
Call window: default constants
--FILE--
<?php
const X=7;function f($a=X+2,$b=['x'=>X*2]){var_dump($a,$b);}f();f(b:[3]);
--EXPECT--
int(9)
array(1) {
  ["x"]=>
  int(14)
}
int(9)
array(1) {
  [0]=>
  int(3)
}
