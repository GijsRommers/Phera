--TEST--
Class members: constant-interface
--FILE--
<?php interface Foo {const A=7;} class Bar implements Foo {} var_dump(Bar::A);
--EXPECT--
int(7)
