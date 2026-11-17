--TEST--
method this
--FILE--
<?php
class Box{private $x=7;function f(){return function(){return $this->x;};}}$f=(new Box)->f();var_dump($f());
--EXPECT--
int(7)
