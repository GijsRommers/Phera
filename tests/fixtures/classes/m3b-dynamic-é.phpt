--TEST--
Class members: dynamic-é
--FILE--
<?php $object=new stdClass; $name="é"; $object->$name=7; var_dump($object->$name, $object);
--EXPECT--
int(7)
object(stdClass)#1 (1) {
  ["é"]=>
  int(7)
}
