--TEST--
Class members: static-isset
--FILE--
<?php class Foo {public static $x=2;} var_dump(isset(Foo::$x));
--EXPECT--
bool(true)
