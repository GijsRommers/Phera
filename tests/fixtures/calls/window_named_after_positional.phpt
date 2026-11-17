--TEST--
Call window: named after positional
--FILE--
<?php
function f($a,$b=2,$c=3){var_dump(func_get_args());}f(1,c:7);
--EXPECT--
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(7)
}
