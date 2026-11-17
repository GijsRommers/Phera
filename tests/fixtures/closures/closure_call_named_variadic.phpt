--TEST--
closure call named variadic
--FILE--
<?php
class A{}$f=function($a,...$rest){return [$a,$rest];};var_dump($f->call(new A,a:1,b:2,c:3));
--EXPECT--
array(2) {
  [0]=>
  int(1)
  [1]=>
  array(2) {
    ["b"]=>
    int(2)
    ["c"]=>
    int(3)
  }
}
