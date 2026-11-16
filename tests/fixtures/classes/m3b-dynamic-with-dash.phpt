--TEST--
Class members: dynamic-with-dash
--FILE--
<?php $object=new stdClass; $name="with-dash"; $object->$name=7; var_dump($object->$name, $object);
--EXPECT--
int(7)
object(stdClass)#1 (1) {
  ["with-dash"]=>
  int(7)
}
