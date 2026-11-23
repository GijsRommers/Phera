--TEST--
A WeakMap value closure keeps its key alive only while the closure is held elsewhere
--FILE--
<?php
$map = new WeakMap;
$key = new stdClass;
$weak = WeakReference::create($key);
$callback = $map[$key] = static fn() => $key;
unset($key);

var_dump(gc_collect_cycles(), count($map), $weak->get() === $callback());
unset($callback);
var_dump(gc_collect_cycles(), count($map), $weak->get());
?>
--EXPECT--
int(0)
int(1)
bool(true)
int(2)
int(0)
NULL
