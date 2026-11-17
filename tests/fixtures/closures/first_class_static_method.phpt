--TEST--
first class static method
--FILE--
<?php
class Box{static function f($y){return $y+3;}}$g=Box::f(...);var_dump($g instanceof Closure,$g(4));
--EXPECT--
bool(true)
int(7)
