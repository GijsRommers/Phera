--TEST--
Class members: static-compound
--FILE--
<?php class Foo {public static $x=2;} Foo::$x+=8; var_dump(Foo::$x);
--EXPECT--
int(10)
