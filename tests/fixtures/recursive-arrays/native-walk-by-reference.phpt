--TEST--
Self-referential array passed to walk-by-reference
--FILE--
<?php
$a = [1];
$a[5] = &$a;
array_walk($a, function (&$value, $key) { if ($key === 5) { $value[0] = 2; } });
var_dump($a[0]);
--EXPECT--
int(2)
