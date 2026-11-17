--TEST--
Class members: dynamic-012
--FILE--
<?php $object=new stdClass; $name="012"; $object->$name=7; var_dump($object->$name, $object);
--EXPECT--
int(7)
object(stdClass)#1 (1) {
  ["012"]=>
  int(7)
}
