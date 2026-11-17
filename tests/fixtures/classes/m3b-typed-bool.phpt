--TEST--
Class members: typed-bool
--FILE--
<?php class Foo {public bool $x;} $a=new Foo; $a->x=1; var_dump($a->x);
--EXPECT--
bool(true)
