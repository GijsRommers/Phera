--TEST--
resolve object array
--FILE--
<?php
class A{function f($x){return $x+1;}}$f=[new A,"f"];var_dump($f(2),call_user_func($f,3));
--EXPECT--
int(3)
int(4)
