--TEST--
Class members: typed-int
--FILE--
<?php class Foo {public int $x;} $a=new Foo; $a->x="12"; var_dump($a->x);
--EXPECT--
int(12)
