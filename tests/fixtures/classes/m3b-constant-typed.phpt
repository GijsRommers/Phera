--TEST--
Class members: constant-typed
--FILE--
<?php class Foo {const int A=7;} var_dump(Foo::A);
--EXPECT--
int(7)
