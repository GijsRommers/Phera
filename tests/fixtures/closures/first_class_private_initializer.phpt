--TEST--
first class private initializer
--FILE--
<?php
class A{private static function f(){return 3;}public $callback=self::f(...);}$a=new A;$f=$a->callback;var_dump($f());
--EXPECT--
int(3)
