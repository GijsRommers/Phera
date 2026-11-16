--TEST--
Class members: constant-external
--FILE--
<?php class Foo {const A=3;} class Bar {const B=Foo::A+4;} var_dump(Bar::B);
--EXPECT--
int(7)
