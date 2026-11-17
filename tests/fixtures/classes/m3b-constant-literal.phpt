--TEST--
Class members: constant-literal
--FILE--
<?php class Foo {const A=42;} var_dump(Foo::A);
--EXPECT--
int(42)
