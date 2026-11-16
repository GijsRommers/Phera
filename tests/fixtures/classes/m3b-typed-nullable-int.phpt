--TEST--
Class members: typed-nullable-int
--FILE--
<?php class Foo {public ?int $x;} $a=new Foo; $a->x=null; var_dump($a->x);
--EXPECT--
NULL
