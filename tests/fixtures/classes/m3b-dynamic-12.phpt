--TEST--
Class members: dynamic-12
--FILE--
<?php $object=new stdClass; $name="12"; $object->$name=7; var_dump($object->$name, $object);
--EXPECT--
int(7)
object(stdClass)#1 (1) {
  ["12"]=>
  int(7)
}
