--TEST--
Class members: constant-arithmetic
--FILE--
<?php class Foo {const A=2+3*4;} var_dump(Foo::A);
--EXPECT--
int(14)
