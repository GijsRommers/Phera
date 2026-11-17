--TEST--
first class object method
--FILE--
<?php
class Box{private $x=3;function f($y){return $this->x+$y;}}$g=(new Box)->f(...);var_dump($g instanceof Closure,$g(4));
--EXPECT--
bool(true)
int(7)
