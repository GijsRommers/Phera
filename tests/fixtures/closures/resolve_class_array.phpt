--TEST--
resolve class array
--FILE--
<?php
class A{static function f($x){return $x+1;}}$f=["A","f"];var_dump($f(2),call_user_func($f,3));
--EXPECT--
int(3)
int(4)
