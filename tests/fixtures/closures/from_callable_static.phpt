--TEST--
from callable static
--FILE--
<?php
class Box{static function add($v){return $v+2;}}$f=Closure::fromCallable([Box::class,"add"]);var_dump($f(4));
--EXPECT--
int(6)
