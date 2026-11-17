--TEST--
invoke variadic named
--FILE--
<?php
class Add{function __invoke($a,...$rest){return [$a,$rest];}}$f=new Add;var_dump($f(a:1,b:2,c:3));
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
