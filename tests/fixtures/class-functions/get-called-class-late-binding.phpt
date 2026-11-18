--TEST--
Get called class late binding
--FILE--
<?php
class A { static function f() { return get_called_class(); } } class B extends A {} var_dump(B::f());
--EXPECT--
string(1) "B"
