--TEST--
Class members: typed-unset
--FILE--
<?php class Foo {public int $x=1;} $a=new Foo; unset($a->x); var_dump(isset($a->x));
--EXPECT--
bool(false)
