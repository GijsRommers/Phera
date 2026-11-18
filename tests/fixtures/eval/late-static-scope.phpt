--TEST--
Late static scope
--FILE--
<?php
class A { public static function f() { return eval('return static::class;'); } } class B extends A {} var_dump(B::f());
--EXPECT--
string(1) "B"
