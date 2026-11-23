--TEST--
Object vars preserves shared reference
--FILE--
<?php
$value = 1;
$object = new stdClass;
$object->value =& $value;
$properties = get_object_vars($object);
$properties["value"] = 2;
var_dump($value);
--EXPECT--
int(2)
