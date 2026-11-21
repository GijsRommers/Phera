--TEST--
Two WeakMaps cannot keep each other's keys alive
--FILE--
<?php
$left = new WeakMap;
$right = new WeakMap;
$a = new stdClass;
$b = new stdClass;
$left[$a] = $b;
$right[$b] = $a;
$refA = WeakReference::create($a);
$refB = WeakReference::create($b);
unset($a, $b);
var_dump(gc_collect_cycles(), count($left), count($right));
var_dump($refA->get() === null, $refB->get() === null);
var_dump(gc_collect_cycles());
--EXPECT--
int(2)
int(0)
int(0)
bool(true)
bool(true)
int(0)
