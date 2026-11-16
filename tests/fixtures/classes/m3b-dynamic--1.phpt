--TEST--
Class members: dynamic--1
--FILE--
<?php $object=new stdClass; $name="-1"; $object->$name=7; var_dump($object->$name, $object);
--EXPECT--
int(7)
object(stdClass)#1 (1) {
  ["-1"]=>
  int(7)
}
