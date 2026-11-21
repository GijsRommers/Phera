--TEST--
Reachability through entries of the same WeakMap reaches a fixed point
--FILE--
<?php
$map = new WeakMap;
$seed = new stdClass;
$a = new stdClass;
$b = new stdClass;
$map[$b] = $a;
$map[$a] = $b;
$map[$seed] = $a;
$refA = WeakReference::create($a);
$refB = WeakReference::create($b);
unset($a, $b);
var_dump(gc_collect_cycles(), count($map));
var_dump($refA->get() !== null, $refB->get() !== null);
unset($seed);
var_dump(gc_collect_cycles(), count($map));
var_dump($refA->get() === null, $refB->get() === null);
--EXPECT--
int(0)
int(3)
bool(true)
bool(true)
int(2)
int(0)
bool(true)
bool(true)
