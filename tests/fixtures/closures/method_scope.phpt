--TEST--
method scope
--FILE--
<?php
class Box{private static $x=9;static function f(){return function(){return self::$x;};}}$f=Box::f();var_dump($f());
--EXPECT--
int(9)
