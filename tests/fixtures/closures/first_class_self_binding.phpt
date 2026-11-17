--TEST--
first class self binding
--FILE--
<?php
class A{static function n(){return static::class;}static function f(){return self::n(...);}}class B extends A{}$g=B::f();var_dump($g());
--EXPECT--
string(1) "B"
