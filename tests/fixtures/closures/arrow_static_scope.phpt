--TEST--
arrow static scope
--FILE--
<?php
class Box{private static $x=3;static function f(){return static fn()=>self::$x;}}$f=Box::f();var_dump($f());
--EXPECT--
int(3)
