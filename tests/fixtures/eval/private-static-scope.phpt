--TEST--
Private static scope
--FILE--
<?php
class A { private static $x = 4; function f() { return eval('return self::$x;'); } } var_dump((new A)->f());
--EXPECT--
int(4)
