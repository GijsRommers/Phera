--TEST--
Class members: typed-string
--FILE--
<?php class Foo {public string $x;} $a=new Foo; $a->x=7; var_dump($a->x);
--EXPECT--
string(1) "7"
