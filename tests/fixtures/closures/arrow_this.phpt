--TEST--
arrow this
--FILE--
<?php
class Box{private $x=3;function f(){return fn($a)=>$this->x+$a;}}$f=(new Box)->f();var_dump($f(4));
--EXPECT--
int(7)
