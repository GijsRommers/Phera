--TEST--
Class members: constant-global
--FILE--
<?php const GLOBAL_VALUE=8; class Foo {const A=GLOBAL_VALUE+1;} var_dump(Foo::A);
--EXPECT--
int(9)
