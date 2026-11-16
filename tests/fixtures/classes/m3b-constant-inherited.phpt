--TEST--
Class members: constant-inherited
--FILE--
<?php class Foo {const A=3;} class Bar extends Foo {} var_dump(Bar::A);
--EXPECT--
int(3)
