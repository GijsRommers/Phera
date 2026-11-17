--TEST--
closure callback map
--FILE--
<?php
$x=3;var_dump(array_map(function($v)use($x){return $v*$x;},[1,2,3]));
--EXPECT--
array(3) {
  [0]=>
  int(3)
  [1]=>
  int(6)
  [2]=>
  int(9)
}
