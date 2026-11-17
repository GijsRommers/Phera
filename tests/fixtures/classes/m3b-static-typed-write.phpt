--TEST--
Class members: static-typed-write
--FILE--
<?php class Foo {public static int $x;} Foo::$x="12"; var_dump(Foo::$x);
--EXPECT--
int(12)
