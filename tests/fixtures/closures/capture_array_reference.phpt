--TEST--
capture array reference
--FILE--
<?php
$a=[1];$f=function()use(&$a){$a[]=2;};$f();$f();var_dump($a);
--EXPECT--
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(2)
}
