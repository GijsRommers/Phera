--TEST--
Class members: constant-function
--FILE--
<?php class Foo {const A=3;} var_dump(constant("Foo::A"));
--EXPECT--
int(3)
