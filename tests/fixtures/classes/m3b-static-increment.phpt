--TEST--
Class members: static-increment
--FILE--
<?php class Foo {public static $x=2;} Foo::$x++; var_dump(Foo::$x);
--EXPECT--
int(3)
