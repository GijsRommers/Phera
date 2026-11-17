--TEST--
Class members: object-class-name
--FILE--
<?php class Foo {} $a=new Foo; var_dump($a::class);
--EXPECT--
string(3) "Foo"
