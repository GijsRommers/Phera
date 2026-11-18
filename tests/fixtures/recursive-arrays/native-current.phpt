--TEST--
Self-referential array passed to current
--FILE--
<?php
$a = [1];
$a[5] = &$a;
end($a);
var_dump(current($a) === $a);
--EXPECT--
bool(true)
