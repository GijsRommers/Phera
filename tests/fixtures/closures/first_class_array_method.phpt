--TEST--
first class array method
--FILE--
<?php
class A{function f($x){return $x+2;}}$f=[new A,"f"];$g=$f(...);var_dump($g instanceof Closure,$g(3));
--EXPECT--
bool(true)
int(5)
