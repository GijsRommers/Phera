--TEST--
parameters variadic
--FILE--
<?php
$f=function($a,...$rest){return [$a,$rest];};var_dump($f(1,2,3));
--EXPECT--
array(2) {
  [0]=>
  int(1)
  [1]=>
  array(2) {
    [0]=>
    int(2)
    [1]=>
    int(3)
  }
}
