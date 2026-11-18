--TEST--
Self-referential array passed to walk
--FILE--
<?php
$a = [1];
$a[5] = &$a;
array_walk($a, function ($value, $key) use (&$a) { var_dump($key, $key === 5 ? $value === $a : $value); });
--EXPECT--
int(0)
int(1)
int(5)
bool(true)
