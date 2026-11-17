--TEST--
arrow snapshot array
--FILE--
<?php
$x=[1,2];$f=fn()=>$x;$x[]=3;var_dump($f());
--EXPECT--
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
