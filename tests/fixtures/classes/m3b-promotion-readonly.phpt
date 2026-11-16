--TEST--
Class members: promotion-readonly
--FILE--
<?php class Foo {function __construct(public readonly int $x=7){}} var_dump(new Foo);
--EXPECT--
object(Foo)#1 (1) {
  ["x"]=>
  int(7)
}
