--TEST--
Class members: typed-int-string
--FILE--
<?php class Foo {public int|string $x;} $a=new Foo; $a->x="a"; var_dump($a->x);
--EXPECT--
string(1) "a"
