--TEST--
A WeakMap value cannot keep its own key alive
--FILE--
<?php
$map = new WeakMap;
$key = new stdClass;
$map[$key] = $key;
$ref = WeakReference::create($key);
unset($key);
var_dump(gc_collect_cycles(), count($map), $ref->get() === null);
var_dump(gc_collect_cycles());
--EXPECT--
int(1)
int(0)
bool(true)
int(0)
