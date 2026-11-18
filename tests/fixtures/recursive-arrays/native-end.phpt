--TEST--
Self-referential array passed to end
--FILE--
<?php
$a = [1];
$a[5] = &$a;
var_dump(end($a) === $a, key($a));
--EXPECT--
bool(true)
int(5)
