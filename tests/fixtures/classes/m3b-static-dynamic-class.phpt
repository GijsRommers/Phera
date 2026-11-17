--TEST--
Class members: static-dynamic-class
--FILE--
<?php class Foo {public static $x=2;} $class='Foo'; var_dump($class::$x);
--EXPECT--
int(2)
