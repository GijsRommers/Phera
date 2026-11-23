--TEST--
A WeakMap entry cannot keep its own map alive even with a live key
--FILE--
<?php
$map = new WeakMap;
$key = new stdClass;
$value = new stdClass;
$map[$key] = [$map, $value];
$refMap = WeakReference::create($map);
$refValue = WeakReference::create($value);
unset($map, $value);
var_dump(gc_collect_cycles(), $refMap->get() === null, $refValue->get() === null);
var_dump(get_debug_type($key), gc_collect_cycles());
--EXPECT--
int(3)
bool(true)
bool(true)
string(8) "stdClass"
int(0)
