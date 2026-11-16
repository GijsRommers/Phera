--TEST--
Class members: dynamic-0
--FILE--
<?php $object=new stdClass; $name="0"; $object->$name=7; var_dump($object->$name, $object);
--EXPECT--
int(7)
object(stdClass)#1 (1) {
  ["0"]=>
  int(7)
}
