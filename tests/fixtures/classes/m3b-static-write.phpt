--TEST--
Class members: static-write
--FILE--
<?php class Foo {public static $x=2;} Foo::$x=9; var_dump(Foo::$x);
--EXPECT--
int(9)
