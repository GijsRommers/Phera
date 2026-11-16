--TEST--
Class members: constant-self
--FILE--
<?php class Foo {const A=3;const B=self::A+4;} var_dump(Foo::B);
--EXPECT--
int(7)
