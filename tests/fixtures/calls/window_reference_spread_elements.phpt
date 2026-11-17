--TEST--
Call window: reference spread elements
--FILE--
<?php
function f(&$a,&$b){$a++;$b*=2;}$a=[3,4];f(...$a);var_dump($a);
--EXPECT--
array(2) {
  [0]=>
  int(4)
  [1]=>
  int(8)
}
