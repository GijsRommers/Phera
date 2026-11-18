--TEST--
Called class scope
--FILE--
<?php
class A { public static function f() { return eval('return get_called_class();'); } } class B extends A {} var_dump(B::f());
--EXPECT--
string(1) "B"
