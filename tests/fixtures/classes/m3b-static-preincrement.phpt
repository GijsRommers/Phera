--TEST--
Class members: static-preincrement
--FILE--
<?php class Foo {public static $x=2;} var_dump(++Foo::$x);
--EXPECT--
int(3)
