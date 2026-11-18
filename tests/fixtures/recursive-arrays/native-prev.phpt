--TEST--
Self-referential array passed to prev
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$a[] = 2;
end($a);
var_dump(prev($a) === $a, key($a));
--EXPECT--
bool(true)
int(5)
