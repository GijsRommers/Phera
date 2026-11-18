--TEST--
Self-referential array passed to reset
--FILE--
<?php
$a = [];
$a[0] = &$a;
$a[] = 1;
end($a);
var_dump(reset($a) === $a, key($a));
--EXPECT--
bool(true)
int(0)
