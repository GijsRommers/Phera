--TEST--
ReflectionObject reports a live dynamic property
--FILE--
<?php
class Entity {}
$object = new Entity();
$object->label = 'test';
$r = new ReflectionObject($object);
var_dump($r->hasProperty('label'), $r->getProperty('label')->isDefault());
var_dump($r->getProperty('label')->getValue($object));
--EXPECTF--
Deprecated: Creation of dynamic property Entity::$label is deprecated in %s on line 4
bool(true)
bool(false)
string(4) "test"
