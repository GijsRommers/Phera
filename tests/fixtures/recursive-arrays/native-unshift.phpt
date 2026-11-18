--TEST--
Self-referential array passed to unshift
--FILE--
<?php
$a = [1];
$a[5] = &$a;
var_dump(array_unshift($a, $a), count($a[0]), $a[2] === $a, key($a));
--EXPECT--
int(3)
int(2)
bool(true)
int(0)
