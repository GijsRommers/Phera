--TEST--
Class members: constant-dynamic-class
--FILE--
<?php class Foo {const A=3;} $name="Foo"; var_dump($name::A);
--EXPECT--
int(3)
