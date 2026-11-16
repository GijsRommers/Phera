--TEST--
Class members: constant-object
--FILE--
<?php class Foo {const A=3;} $object=new Foo; var_dump($object::A);
--EXPECT--
int(3)
