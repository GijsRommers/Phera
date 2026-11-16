--TEST--
Class members: static-reference
--FILE--
<?php class Foo {public static $x=2;} $ref=&Foo::$x; $ref=6; var_dump(Foo::$x);
--EXPECT--
int(6)
