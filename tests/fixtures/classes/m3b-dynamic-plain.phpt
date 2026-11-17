--TEST--
Class members: dynamic-plain
--FILE--
<?php $object=new stdClass; $name="plain"; $object->$name=7; var_dump($object->$name, $object);
--EXPECT--
int(7)
object(stdClass)#1 (1) {
  ["plain"]=>
  int(7)
}
