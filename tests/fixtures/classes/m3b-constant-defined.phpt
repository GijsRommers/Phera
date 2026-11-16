--TEST--
Class members: constant-defined
--FILE--
<?php class Foo {const A=3;} var_dump(defined("Foo::A"),defined("Foo::B"));
--EXPECT--
bool(true)
bool(false)
