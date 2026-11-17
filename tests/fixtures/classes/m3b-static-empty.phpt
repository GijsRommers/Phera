--TEST--
Class members: static-empty
--FILE--
<?php class Foo {public static $x=2;} var_dump(empty(Foo::$x));
--EXPECT--
bool(false)
