--TEST--
Class members: static-typed-isset
--FILE--
<?php class Foo {public static int $x;} var_dump(isset(Foo::$x));
--EXPECT--
bool(false)
