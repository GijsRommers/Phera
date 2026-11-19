--TEST--
WeakReference and WeakMap release weak keys when their owners go away
--FILE--
<?php
$key = new stdClass;
$map = new WeakMap;
$map[$key] = 'value';
$ref = WeakReference::create($key);
var_dump($ref === WeakReference::create($key), $ref->get() === $key);
foreach ($map as $object => $value) {
    var_dump($object === $key, $value);
}
unset($object, $key);
var_dump($ref->get(), $map->count());
--EXPECT--
bool(true)
bool(true)
bool(true)
string(5) "value"
NULL
int(0)
