--TEST--
Self-referential array passed to next
--FILE--
<?php
$a = [1];
$a[5] = &$a;
var_dump(next($a) === $a, key($a));
--EXPECT--
bool(true)
int(5)
