--TEST--
Self-referential array passed to pos
--FILE--
<?php
$a = [1];
$a[5] = &$a;
end($a);
var_dump(pos($a) === $a);
--EXPECT--
bool(true)
