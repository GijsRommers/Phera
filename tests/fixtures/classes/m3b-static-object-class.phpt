--TEST--
Class members: static-object-class
--FILE--
<?php class Foo {public static $x=2;} $object=new Foo; var_dump($object::$x);
--EXPECT--
int(2)
