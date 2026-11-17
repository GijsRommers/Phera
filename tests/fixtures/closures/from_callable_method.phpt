--TEST--
from callable method
--FILE--
<?php
class Box{private $x=5;function add($v){return $this->x+$v;}}$f=Closure::fromCallable([new Box,"add"]);var_dump($f(4));
--EXPECT--
int(9)
