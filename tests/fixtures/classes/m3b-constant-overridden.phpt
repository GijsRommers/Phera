--TEST--
Class members: constant-overridden
--FILE--
<?php class Foo {const A=3;} class Bar extends Foo {const A=4;} var_dump(Foo::A,Bar::A);
--EXPECT--
int(3)
int(4)
