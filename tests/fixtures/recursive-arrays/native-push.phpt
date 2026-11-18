--TEST--
Self-referential array passed to push
--FILE--
<?php
$a = [1];
$a[5] = &$a;
var_dump(array_push($a, $a), count($a[6]), $a[5] === $a);
--EXPECT--
int(3)
int(2)
bool(true)
