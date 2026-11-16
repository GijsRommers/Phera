--TEST--
Class members: constant-private-self
--FILE--
<?php class Foo {private const A=7;public const B=self::A;} var_dump(Foo::B);
--EXPECT--
int(7)
