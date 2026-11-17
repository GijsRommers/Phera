--TEST--
Class members: dynamic-a-b
--FILE--
<?php $object=new stdClass; $name="a b"; $object->$name=7; var_dump($object->$name, $object);
--EXPECT--
int(7)
object(stdClass)#1 (1) {
  ["a b"]=>
  int(7)
}
