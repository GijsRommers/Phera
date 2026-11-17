--TEST--
arrow map
--FILE--
<?php
$n=3;var_dump(array_map(fn($x)=>$x*$n,[1,2,3]));
--EXPECT--
array(3) {
  [0]=>
  int(3)
  [1]=>
  int(6)
  [2]=>
  int(9)
}
